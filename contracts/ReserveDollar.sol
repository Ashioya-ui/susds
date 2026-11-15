// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

/// @title Pro Rata Reserve Dollar: a dollar coin meant to be held as a reserve.
/// @notice Backed by n dollar coins from independent issuers, in equal parts. No owner, no pause,
///   no upgrade, no price feed. Two claims on the reserve: the coin (redeemable at par) and
///   buffer shares (first loss, earn the 0.10% mint fee). All amounts are in 6-decimal dollars.
///
///   SAFE: for every leg i,  total - d[i] * h[i] >= supply,
///   where d[i] is the largest loss the contract is built to survive on leg i (set at deployment,
///   100% = the leg may fail completely).
///
///   Theorem 1. If SAFE holds and leg i loses at most d[i] of its value, the reserve is still worth
///     at least one dollar per coin. With d[i] = 100% every coin can be redeemed at par from the
///     other legs alone, because redeem lets x coins take up to x*h[k]/supply from each leg k.
///   Theorem 2. redeem never lowers h[k]/supply for any leg, so redeeming early takes nothing from
///     those who redeem later, and SAFE survives every redemption.
///   Theorem 3. mint and withdrawBuffer revert unless SAFE holds afterwards; fund only adds reserve.
///     So no sequence of calls takes the contract from SAFE to not SAFE.
///   Theorem 4. Equity per buffer share never falls through any call.
///   With n equal legs and d = 100% the buffer needed is 1/(n-1) of supply: the more independent
///   issuers, the cheaper the coin is to keep safe. The proofs and a 600,000-call randomized check
///   of the same rules are in near/prusd/src/core.rs.
contract ReserveDollar {
    string public constant name = "Pro Rata Reserve Dollar";
    string public constant symbol = "PRRD";
    uint8 public constant decimals = 6;
    uint256 public constant FEE_BPS = 10;

    address[] public legs;
    uint256[] public scale;      // token units per 6-decimal dollar unit
    uint256[] public dBps;       // largest loss survived on each leg, basis points
    uint256[] public h;          // holdings per leg, 6-decimal dollars
    uint256 public totalSupply;
    uint256 public bufferShares;
    mapping(address => uint256) public balanceOf;
    mapping(address => uint256) public bufferSharesOf;
    mapping(address => mapping(address => uint256)) public allowance;
    uint256 private lock = 1;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event Mint(address indexed to, uint256 coins, uint256 perLeg);
    event Redeem(address indexed from, uint256 coins, uint256[] take);
    event Fund(address indexed from, uint256 amount, uint256 shares);
    event BufferWithdrawn(address indexed to, uint256 shares, uint256[] outs);

    modifier guarded() { require(lock == 1, "reentry"); lock = 2; _; lock = 1; }

    constructor(address[] memory legs_, uint8[] memory tokenDecimals, uint256[] memory dBps_) {
        uint256 n = legs_.length;
        require(n >= 2 && n <= 8 && tokenDecimals.length == n && dBps_.length == n, "legs");
        for (uint256 i; i < n; i++) {
            for (uint256 k; k < i; k++) require(legs_[k] != legs_[i], "legs must differ");
            require(tokenDecimals[i] >= 6 && tokenDecimals[i] <= 18 && dBps_[i] > 0 && dBps_[i] <= 10_000, "leg");
            legs.push(legs_[i]); scale.push(10 ** (tokenDecimals[i] - 6)); dBps.push(dBps_[i]); h.push(0);
        }
    }

    // ---- views ------------------------------------------------------------------------------
    function legCount() public view returns (uint256) { return legs.length; }
    function holdings() external view returns (uint256[] memory) { return h; }
    function total() public view returns (uint256 t) { for (uint256 i; i < h.length; i++) t += h[i]; }
    function equity() public view returns (uint256) { uint256 t = total(); return t > totalSupply ? t - totalSupply : 0; }
    function safe() public view returns (bool) {
        uint256 t = total();
        for (uint256 i; i < h.length; i++) if (t - h[i] * dBps[i] / 10_000 < totalSupply) return false;
        return true;
    }
    /// A valid `take` for redeeming x that leaves out leg `skip` (pass legCount() to skip none).
    function quoteRedeem(uint256 x, uint256 skip) external view returns (uint256[] memory take) {
        uint256 n = h.length; take = new uint256[](n);
        if (totalSupply == 0 || x == 0 || x > totalSupply) return take;
        uint256 left = x;
        for (uint256 k; k < n && left > 0; k++) {
            if (k == skip) continue;
            uint256 t = x * h[k] / totalSupply; if (t > left) t = left;
            take[k] = t; left -= t;
        }
    }

    // ---- the coin ---------------------------------------------------------------------------
    /// Mint `coins`: pays coins/n plus 0.10% in every leg. Reverts unless SAFE holds afterwards.
    function mint(uint256 coins) external guarded {
        uint256 n = h.length;
        require(coins > 0 && coins % n == 0, "amount must be a positive multiple of the number of legs");
        uint256 per = coins / n; per += (per * FEE_BPS + 9_999) / 10_000;
        for (uint256 i; i < n; i++) { _pull(i, per); h[i] += per; }
        totalSupply += coins; balanceOf[msg.sender] += coins;
        require(safe(), "mint gate: the buffer is too small for this mint; fund the buffer first");
        emit Transfer(address(0), msg.sender, coins); emit Mint(msg.sender, coins, per);
    }

    /// Burn `coins` and take `take[k]` from leg k: at most `coins` in total, at most coins*h[k]/supply per leg.
    function redeem(uint256 coins, uint256[] calldata take) external guarded {
        uint256 n = h.length; uint256 s = totalSupply;
        require(coins > 0 && balanceOf[msg.sender] >= coins && take.length == n, "amount");
        uint256 sum;
        for (uint256 k; k < n; k++) { require(take[k] <= coins * h[k] / s, "more than the pro-rata share of a leg"); sum += take[k]; }
        require(sum <= coins, "more than par");
        balanceOf[msg.sender] -= coins; totalSupply = s - coins;
        for (uint256 k; k < n; k++) { h[k] -= take[k]; }
        for (uint256 k; k < n; k++) { _push(k, take[k]); }
        emit Transfer(msg.sender, address(0), coins); emit Redeem(msg.sender, coins, take);
    }

    // ---- the buffer -------------------------------------------------------------------------
    /// Add `amount` to the buffer in equal parts and receive buffer shares.
    function fund(uint256 amount) external guarded returns (uint256 shares) {
        uint256 n = h.length;
        require(amount > 0 && amount % n == 0, "amount must be a positive multiple of the number of legs");
        uint256 e = equity();
        if (bufferShares == 0) shares = amount; else { require(e > 0, "buffer equity is zero"); shares = amount * bufferShares / e; }
        require(shares > 0, "amount too small");
        for (uint256 i; i < n; i++) { _pull(i, amount / n); h[i] += amount / n; }
        bufferShares += shares; bufferSharesOf[msg.sender] += shares;
        emit Fund(msg.sender, amount, shares);
    }

    /// Burn buffer shares for their share of equity, paid in proportion to holdings. Reverts unless SAFE holds afterwards.
    function withdrawBuffer(uint256 shares) external guarded {
        uint256 n = h.length; uint256 t = total();
        require(shares > 0 && bufferSharesOf[msg.sender] >= shares && t > 0, "amount");
        uint256 v = shares * equity() / bufferShares;
        uint256[] memory outs = new uint256[](n);
        for (uint256 k; k < n; k++) { outs[k] = v * h[k] / t; }
        for (uint256 k; k < n; k++) { h[k] -= outs[k]; }
        bufferShares -= shares; bufferSharesOf[msg.sender] -= shares;
        require(safe(), "buffer locked: this withdrawal would leave the coin exposed");
        for (uint256 k; k < n; k++) { _push(k, outs[k]); }
        emit BufferWithdrawn(msg.sender, shares, outs);
    }

    // ---- ERC-20 -----------------------------------------------------------------------------
    function transfer(address to, uint256 value) external returns (bool) { _move(msg.sender, to, value); return true; }
    function approve(address spender, uint256 value) external returns (bool) { allowance[msg.sender][spender] = value; emit Approval(msg.sender, spender, value); return true; }
    function transferFrom(address from, address to, uint256 value) external returns (bool) {
        uint256 a = allowance[from][msg.sender];
        if (a != type(uint256).max) allowance[from][msg.sender] = a - value;
        _move(from, to, value); return true;
    }
    function _move(address from, address to, uint256 value) internal {
        require(to != address(0), "to");
        balanceOf[from] -= value; balanceOf[to] += value; emit Transfer(from, to, value);
    }

    // ---- tokens (works with coins that return nothing, such as USDT) --------------------------
    function _pull(uint256 i, uint256 amount) internal {
        (bool ok, bytes memory r) = legs[i].call(abi.encodeWithSelector(0x23b872dd, msg.sender, address(this), amount * scale[i]));
        require(ok && (r.length == 0 || abi.decode(r, (bool))), "pull");
    }
    function _push(uint256 i, uint256 amount) internal {
        if (amount == 0) return;
        (bool ok, bytes memory r) = legs[i].call(abi.encodeWithSelector(0xa9059cbb, msg.sender, amount * scale[i]));
        require(ok && (r.length == 0 || abi.decode(r, (bool))), "push");
    }
}
