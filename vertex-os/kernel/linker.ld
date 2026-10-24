/* Vertex OS Kernel Linker Script
 *
 * The kernel is linked into the higher half at 0xffffffff80000000, which is
 * Limine's default kernel load address. Layout requirements:
 *
 *   - `.requests*` must be KEEP'd, writable and contiguous: Limine walks that
 *     range to find its request structures and writes responses into them.
 *   - `__kernel_start` / `__kernel_end` are used by the physical memory
 *     manager to reserve the kernel image.
 */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

PHDRS
{
    text   PT_LOAD FLAGS(0x05);  /* R-X */
    rodata PT_LOAD FLAGS(0x04);  /* R-- */
    data   PT_LOAD FLAGS(0x06);  /* RW- */
}

SECTIONS
{
    . = 0xffffffff80000000;
    __kernel_start = .;

    .text : {
        *(.text .text.*)
    } :text

    . = ALIGN(4096);

    .rodata : {
        *(.rodata .rodata.* .srodata .srodata.*)
    } :rodata

    . = ALIGN(4096);

    .data : {
        /* Limine requests: must stay contiguous and writable. */
        KEEP(*(.requests_start))
        KEEP(*(.requests))
        KEEP(*(.requests_end))
        *(.data .data.* .sdata .sdata.*)
        *(.got .got.plt)
    } :data

    .bss : {
        *(COMMON)
        *(.bss .bss.* .sbss .sbss.*)
    } :data

    . = ALIGN(4096);
    __kernel_end = .;

    /DISCARD/ : {
        *(.eh_frame*)
        *(.note .note.*)
        *(.comment)
        *(.interp)
    }
}
