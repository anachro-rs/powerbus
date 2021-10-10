# Bootloader

Okay, I have some other notes, but it's time to get practical.

I need three interfaces/behaviors for the bootloader code:

1. Bootloader
2. Boot data
3. Application

## Bootloader

The bootloader should basically do this:

* Check if a new image was flashed (slot A or slot B)
    * If so, validate the image (poly1305, sanity)
    * If good, copy image to active
* Otherwise, check if we failed to hit "good" on last boot
    * Check if older image is good
    * If so, copy OLDER image to active (if there is one)
    * If not, just hold in bootloader
* Decide which image to provide for overwrite
    * If just flashed, do the older one
    * If failure, do the newer one
    * Otherwise, do the older one
* Boot

## Boot data

* Contains some pointers and metadata
* Ptr to current header, for marking "good"
* Ptr to a section available for overwrite
* Maybe some flags for:
    * Is first boot
    * Is rollback

## App

* If first boot, wait a bit (maybe until connection?), then mark good
* If rollback, maybe tell someone about it?
* Wait for bootload command
    * First, get header, determine how many pages needed
* Foreach page
    * Erase page (step at a time)
    * Foreach chunk (16/page):
        * Ask for page
        * Receive page
        * Check page checksum
        * Write page (step at a time)
* Say farewell
* Reboot to bootloader
