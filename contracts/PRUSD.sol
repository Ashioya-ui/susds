// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

/// @title Pro Rata Dollar (PRUSD): a stablecoin with an atomic par link and pro-rata redemption.
/// @notice One contract, no owner, no pause, no upgrade. The reserve is USDC held here.
///   D1 (atomic par): mint and redeem are one call each, at every hour, against the reserve.
///   D2 (pro rata):   a redemption of x coins pays x * min(1, B) USDC, B = reserve / supply,
///                    so the first coin redeemed receives the same as the last.
///   D3 (buffer):     anyone may add USDC to the reserve without minting; the excess over the
///                    supply is the first-loss buffer b = B - 1.
/// Invariant: after any mint, redeem or fund, reserve >= supply whenever it was before
/// (Lemma 7 of "Depegs are holes").
interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
    function balanceOf(address account) external view returns (uint256);
}

contract PRUSD {
    string public constant name = "Pro Rata Dollar";
    string public constant symbol = "PRUSD";
    uint8 public constant decimals = 6;
    IERC20 public immutable usdc;

    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event Mint(address indexed to, uint256 coins);
    event Redeem(address indexed from, uint256 coins, uint256 usdcOut);
    event Fund(address indexed from, uint256 usdcIn);

    constructor(IERC20 usdc_) {
        usdc = usdc_;
    }

    // ---- reserve ----------------------------------------------------------------------------

    function reserve() public view returns (uint256) {
        return usdc.balanceOf(address(this));
    }

    /// @return Backing ratio B in parts per million (1e6 = one dollar of reserve per coin).
    function backingPpm() external view returns (uint256) {
        if (totalSupply == 0) return 1e6;
        return reserve() * 1e6 / totalSupply;
    }

    /// @notice Buffer b = B - 1 in basis points, 0 when the reserve does not exceed the supply.
    function bufferBps() external view returns (uint256) {
        uint256 r = reserve();
        if (totalSupply == 0 || r <= totalSupply) return 0;
        return (r - totalSupply) * 10_000 / totalSupply;
    }

    /// @notice Add USDC to the reserve without minting: the first-loss buffer.
    function fund(uint256 usdcIn) external {
        require(usdc.transferFrom(msg.sender, address(this), usdcIn), "usdc");
        emit Fund(msg.sender, usdcIn);
    }

    // ---- par link ---------------------------------------------------------------------------

    /// @notice One USDC in, one coin out. Keeps B >= 1 if it was.
    function mint(uint256 coins) external {
        require(usdc.transferFrom(msg.sender, address(this), coins), "usdc");
        totalSupply += coins;
        balanceOf[msg.sender] += coins;
        emit Transfer(address(0), msg.sender, coins);
        emit Mint(msg.sender, coins);
    }

    /// @notice Burn coins, receive coins * min(1, B) USDC. Pro rata: B does not fall.
    function redeem(uint256 coins) external returns (uint256 usdcOut) {
        uint256 r = reserve();
        uint256 s = totalSupply;
        usdcOut = r >= s ? coins : coins * r / s;
        balanceOf[msg.sender] -= coins;
        totalSupply = s - coins;
        emit Transfer(msg.sender, address(0), coins);
        require(usdc.transfer(msg.sender, usdcOut), "usdc");
        emit Redeem(msg.sender, coins, usdcOut);
    }

    // ---- ERC-20 -----------------------------------------------------------------------------

    function transfer(address to, uint256 value) external returns (bool) {
        _move(msg.sender, to, value);
        return true;
    }

    function approve(address spender, uint256 value) external returns (bool) {
        allowance[msg.sender][spender] = value;
        emit Approval(msg.sender, spender, value);
        return true;
    }

    function transferFrom(address from, address to, uint256 value) external returns (bool) {
        uint256 a = allowance[from][msg.sender];
        if (a != type(uint256).max) allowance[from][msg.sender] = a - value;
        _move(from, to, value);
        return true;
    }

    function _move(address from, address to, uint256 value) internal {
        require(to != address(0), "to");
        balanceOf[from] -= value;
        balanceOf[to] += value;
        emit Transfer(from, to, value);
    }
}
