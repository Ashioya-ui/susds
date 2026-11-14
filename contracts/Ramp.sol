// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

/// @title Ramp: PRUSD to M-Pesa through bonded agents. No owner, no pause, no upgrade.
/// @notice A buyer sells `amount` PRUSD for M-Pesa shillings paid by an agent off chain.
///   open      buyer locks 2*amount (the amount and an equal stake), names an agent.
///   accept    agent reserves `amount` of its bond against the order; one hour to pay.
///   receipt   agent posts the hash of the M-Pesa receipt code; one hour for the buyer.
///   release   buyer confirms: amount to the agent, stake back to the buyer.
///   refund    no receipt in time: buyer gets 2*amount back plus 1% of amount from the bond.
///   claim     receipt posted, buyer silent for an hour: amount to the agent, stake to the buyer.
///   dispute   buyer rejects the receipt in time: amount, stake and the reserved bond
///             (3*amount) are redeemed and funded into the PRUSD reserve. Neither side
///             collects, so a false receipt costs the agent `amount` and a false dispute
///             costs the buyer `amount`.
///
/// Invariant 1 (locked funds). The 2*amount of an order leaves this contract only in
///   cancel, release, refund, claim or dispute. Proof: those are the only functions that
///   transfer on behalf of an order; each requires state Open, Accepted or Receipted and
///   sets state Closed before any transfer; no function leaves Closed. So each order pays
///   out exactly once, to the parties and in the amounts listed above.
/// Invariant 2 (bond). bond[a] >= reserved[a] for every agent a, at all times.
///   Proof by cases on every write to either mapping:
///   postBond      bond += x.
///   withdrawBond  requires bond - reserved >= x, then bond -= x.
///   accept        requires bond - reserved >= amount, then reserved += amount.
///   release,claim reserved -= amount.
///   refund        bond -= fee, reserved -= amount, and fee <= amount.
///   dispute       bond -= amount, reserved -= amount.
///   Each step preserves the inequality, and reserved[a] is the sum of `amount` over
///   the orders of a in state Accepted or Receipted, so the bond covers every open obligation.
interface IPRUSD {
    function transfer(address to, uint256 amount) external returns (bool);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
    function redeem(uint256 coins) external returns (uint256);
    function fund(uint256 usdcIn) external;
}

interface IUSDC {
    function approve(address spender, uint256 amount) external returns (bool);
}

contract Ramp {
    enum State { None, Open, Accepted, Receipted, Closed }

    struct Order {
        address buyer;
        address agent;
        uint256 amount;      // PRUSD sold, 6 decimals
        uint256 kes;         // shillings the agent must pay
        bytes32 phoneHash;   // keccak256 of the buyer's M-Pesa number and a salt
        bytes32 receiptHash; // keccak256 of the M-Pesa receipt code
        uint64 deadline;
        State state;
    }

    uint256 public constant WINDOW = 1 hours;
    uint256 public constant FEE_BPS = 100;
    IPRUSD public immutable prusd;

    mapping(address => uint256) public bond;
    mapping(address => uint256) public reserved;
    Order[] public orders;

    event Bond(address indexed agent, uint256 bond);
    event Opened(uint256 indexed id, address indexed buyer, address indexed agent, uint256 amount, uint256 kes, bytes32 phoneHash);
    event Accepted(uint256 indexed id, uint64 deadline);
    event Receipt(uint256 indexed id, bytes32 receiptHash, uint64 deadline);
    event Closed(uint256 indexed id, string how);

    constructor(IPRUSD prusd_, IUSDC usdc_) {
        prusd = prusd_;
        usdc_.approve(address(prusd_), type(uint256).max);
    }

    function orderCount() external view returns (uint256) {
        return orders.length;
    }

    // ---- agents -----------------------------------------------------------------------------

    function postBond(uint256 x) external {
        bond[msg.sender] += x;
        require(prusd.transferFrom(msg.sender, address(this), x), "prusd");
        emit Bond(msg.sender, bond[msg.sender]);
    }

    function withdrawBond(uint256 x) external {
        require(bond[msg.sender] - reserved[msg.sender] >= x, "reserved");
        bond[msg.sender] -= x;
        require(prusd.transfer(msg.sender, x), "prusd");
        emit Bond(msg.sender, bond[msg.sender]);
    }

    // ---- orders -----------------------------------------------------------------------------

    function open(address agent, uint256 amount, uint256 kes, bytes32 phoneHash) external returns (uint256 id) {
        require(amount > 0, "amount");
        id = orders.length;
        orders.push(Order(msg.sender, agent, amount, kes, phoneHash, 0, 0, State.Open));
        require(prusd.transferFrom(msg.sender, address(this), 2 * amount), "prusd");
        emit Opened(id, msg.sender, agent, amount, kes, phoneHash);
    }

    function cancel(uint256 id) external {
        Order storage o = orders[id];
        require(o.state == State.Open && (msg.sender == o.buyer || msg.sender == o.agent), "cancel");
        o.state = State.Closed;
        require(prusd.transfer(o.buyer, 2 * o.amount), "prusd");
        emit Closed(id, "cancel");
    }

    function accept(uint256 id) external {
        Order storage o = orders[id];
        require(o.state == State.Open && msg.sender == o.agent, "accept");
        require(bond[msg.sender] - reserved[msg.sender] >= o.amount, "bond");
        reserved[msg.sender] += o.amount;
        o.state = State.Accepted;
        o.deadline = uint64(block.timestamp + WINDOW);
        emit Accepted(id, o.deadline);
    }

    function submitReceipt(uint256 id, bytes32 receiptHash) external {
        Order storage o = orders[id];
        require(o.state == State.Accepted && msg.sender == o.agent && block.timestamp <= o.deadline, "receipt");
        o.state = State.Receipted;
        o.receiptHash = receiptHash;
        o.deadline = uint64(block.timestamp + WINDOW);
        emit Receipt(id, receiptHash, o.deadline);
    }

    function release(uint256 id) external {
        Order storage o = orders[id];
        require((o.state == State.Accepted || o.state == State.Receipted) && msg.sender == o.buyer, "release");
        _settle(id, o, "release");
    }

    function claim(uint256 id) external {
        Order storage o = orders[id];
        require(o.state == State.Receipted && block.timestamp > o.deadline, "claim");
        _settle(id, o, "claim");
    }

    function refund(uint256 id) external {
        Order storage o = orders[id];
        require(o.state == State.Accepted && block.timestamp > o.deadline, "refund");
        uint256 fee = o.amount * FEE_BPS / 10_000;
        o.state = State.Closed;
        bond[o.agent] -= fee;
        reserved[o.agent] -= o.amount;
        require(prusd.transfer(o.buyer, 2 * o.amount + fee), "prusd");
        emit Closed(id, "refund");
    }

    function dispute(uint256 id) external {
        Order storage o = orders[id];
        require(o.state == State.Receipted && msg.sender == o.buyer && block.timestamp <= o.deadline, "dispute");
        o.state = State.Closed;
        bond[o.agent] -= o.amount;
        reserved[o.agent] -= o.amount;
        prusd.fund(prusd.redeem(3 * o.amount));
        emit Closed(id, "dispute");
    }

    function _settle(uint256 id, Order storage o, string memory how) internal {
        o.state = State.Closed;
        reserved[o.agent] -= o.amount;
        require(prusd.transfer(o.agent, o.amount), "prusd");
        require(prusd.transfer(o.buyer, o.amount), "prusd");
        emit Closed(id, how);
    }
}
