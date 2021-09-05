# Layout thoughts

So, let's jump to the end, what I WANT my program to look like:


```rust

static ALLOC: BSlab = BSlab::new();

static IOQ_0: IoQueue = IoQueue::new();
static DISPATCH_0: Dispatch = Dispatch::new(
    &IOQ_0,
    &ALLOC,
);

static IOQ_1: IoQueue = IoQueue::new();
static DISPATCH_1: Dispatch = Dispatch::new(
    &IOQ_1,
    &ALLOC,
);

#[init]
fn init() -> Resources {
    // TODO: I need to:
    // * Set up the UARTEs, timers, etc
    // * Init the allocator

    ALLOC.init().ok();
    GlobalRollingTimer::init(board.TIMER0);

    Resources {

    }
}

#[interrupt]
fn uarte0(ctx) {

}

#[idle]
fn main(ctx) -> ! {
    // TODO: I need to:
    // * Set up the executor(s)
    // * Poll them a bunch
}

```

I also have no idea how I'm going to do the "switch dom and sub roles". Especially the part where we need to be a "sub" on each interface at boot. Or maybe just "have ever received a packet" is enough?

The async stuff makes it hard :(

Maybe put a bool at the top of the event loop, with some kind of ack? "wrap it up"? Some kind of mutex? Note: I probably need a "yield now" after the mutex drop to let someone else try and pick it up.

I'm probably going to need one "Interface" for each port, which contains both the dom and sub parts. On a new entry, it should clear out the queues and such/re-init any state.

# Reset

Okay. The goal is to validate the hardware ASAP.

I should focus on making two firmwares:

One DOM (probably with a PC uplink), and one Sub-only.

That way I can actually validate the entire hardware with only three devices:

* One Dom+Cap (with PC uplink)
* One Sub+Bus
* One Sub+Cap

That's enough to move the hardware design forward. Lets focus on that, before getting caught up in routing and shit. 32 devices on one bus is enough for now.

# Interfaces

For each side, the interface itself will swap between send and receive, never both at
the same time.

## Dom

So, the hardware interface on the Dom side will always follow some kind of pattern:

* Optional: Send one message
* Send a grant token
* Receive with timeout

It can basically just loop forever in that pattern forever, as long as there are packets
to drain - and probably even when there isn't a packet to drain.

I actually might want to have a field on the local packet header that says whether we
should listen for one or more responses. Maybe time based?

This is going to be a pain in the ass for the discovery stuff.

## Sub

The Sub is basically receiving all of the time. The problem is, we don't do any decoding
at the actual interface, to figure out whether we should respond.

More or less, we:

* Listen forever, until
* We get some kind of notification from the app to send a packet

The thing I worry about is for cases where we need to send the RIGHT message as
a response, like during the discovery/enumeration process. That being said, it might
be possible to inhibit all other messages for the discovery case, and then maybe
never assume direct ping/responses after that?

This would eliminate the concept of regular pings though, but I assume we can maybe
include that kind of info in the grant release?
