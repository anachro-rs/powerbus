# Alloc thoughts

Okay, lifetimes are going to suck. I will probably want some kind of byte-buffer based allocator, similar to (redacted ferrous project).

## Interface needs:

* A global/trait based failable allocator
* The abilitity to get a MUTABLE box of bytes initially
* The ability to DMA into the box
* The ability to turn that Box into an Arc
    * This makes it immutable
* The ability to sub-slice the Arc
    * This keeps a common reference count
    * This will mostly be used to keep a payload and avoid copying it around

In the future, I may want the ability to Serialize without copying a payload.

This would be something... odd, where I could break/split the outgoing message into a new header, which has different contents, but keep the payload uncopied, and DMA it out directly.

This would avoid spending time in memcpy for transferring an incoming packet into an outgoing packet.

THAT BEING SAID, that probably isn't possible! At the very least, we'll need to cobs re-encode the data, which is not an in-place operation!

So for now, the process will probably be:

ON INCOMING:

* Alloc a new box
* DMA receive into that box
* Mutably De-cobs in-place
* Convert to an Arc
* Deserialize, with the ability to have fields that hold an Arc of the box
    * I feel like this is going to be *a thing*.
* Hold on to that deserialized item until sending

ON OUTGOING

* Alloc a new box
* Serialize the last box/arc into the new box
* Drop the old box/arc
* DMA send that box
* Drop the new box

This means that BRIEFLY we'll need two boxes for every message we forward. This might be optimizable in the future to do in-place, but would have to have some pretty significant limitations, or be pretty dangerous/complicated unsafe code. Not worth it for now.
