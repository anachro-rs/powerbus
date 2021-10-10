/* NOTE: This is the linker script for the anachro-boot bootloader! */

MEMORY
{
  /* NOTE K = KiBi = 1024 bytes */
/*  FLASH     : ORIGIN = 0x00000000, LENGTH = 256K */
  FLASH       : ORIGIN = 0x00040000, LENGTH = 256K
/*  STORAGE1  : ORIGIN = 0x00080000, LENGTH = 256K */
/*  STORAGE2  : ORIGIN = 0x000C0000, LENGTH = 256K */
  RAM       : ORIGIN = 0x20000000, LENGTH = 256K
}

/* Applications should use:                           */
/*                                                    */
/* MEMORY                                             */
/* {                                                  */
/*   METADATA :   ORIGIN = 0x00040000, LENGTH = 4K    */
/*   FLASH :      ORIGIN = 0x00041000, LENGTH = 252K  */
/*   RAM :        ORIGIN = 0x20000000, LENGTH = 256K  */
/* }                                                  */
/*                                                    */

/* This is where the call stack will be allocated. */
/* The stack is of the full descending type. */
/* You may want to use this variable to locate the call stack and static
   variables in different memory regions. Below is shown the default value */
/* _stack_start = ORIGIN(RAM) + LENGTH(RAM); */

/* You can use this symbol to customize the location of the .text section */
/* If omitted the .text section will be placed right after the .vector_table
   section */
/* This is required only on microcontrollers that store some configuration right
   after the vector table */
/* _stext = ORIGIN(FLASH) + 0x400; */

/* Size of the heap (in bytes) */
/* _heap_size = 1024; */
