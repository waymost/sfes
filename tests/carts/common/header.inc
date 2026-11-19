; Cartridge header and exception vectors shared by the own test cartridges (ADR 0013).
; @source: fullsnes#snescartridgeromheader
; @source: wdc-w65c816s#table-5-2, wdc-w65c816s#table-5-3
; @source: adr-0013
;
; `cargo xtask carts build` writes the ROM size byte ($FFD7) and the checksum and its
; complement ($FFDC-$FFDF) after linking; the macro leaves placeholders there.

; cart_header title, map_mode, country
;   title     up to 21 ASCII characters, padded with spaces to 21 ($FFC0-$FFD4)
;   map_mode  ROM speed and map mode ($FFD5): $20 = slow LoROM, $21 = slow HiROM
;   country   $FFD9: $01 = USA and Canada (NTSC), $02 = Europe (PAL)
; @source: fullsnes#snescartridgeromheader
.macro cart_header title, map_mode, country
    .assert .strlen(title) <= 21, error, "cartridge title longer than 21 characters"
    .pushseg
    .segment "HEADER"
    .byte title
    .res 21 - .strlen(title), $20
    .byte map_mode              ; $FFD5 ROM speed and map mode
    .byte $00                   ; $FFD6 chipset: ROM only
    .byte $00                   ; $FFD7 ROM size, written by cargo xtask carts build
    .byte $00                   ; $FFD8 RAM size: none
    .byte country               ; $FFD9 country
    .byte $00                   ; $FFDA developer: none
    .byte $00                   ; $FFDB version
    .word $FFFF                 ; $FFDC checksum complement, written by cargo xtask carts build
    .word $0000                 ; $FFDE checksum, written by cargo xtask carts build
    .popseg
.endmacro

; cart_vectors reset, nmi, irq, fault
;   reset  the emulation-mode reset vector; the CPU always starts in emulation mode
;   nmi    both NMI vectors
;   irq    the native IRQ vector and the shared emulation-mode IRQ/BRK vector
;   fault  COP, BRK and ABORT in native mode, COP and ABORT in emulation mode
; Reserved entries are zero.
; @source: wdc-w65c816s#table-5-2, wdc-w65c816s#table-5-3, fullsnes#snescartridgeromheader
.macro cart_vectors reset, nmi, irq, fault
    .pushseg
    .segment "VECTORS"
    .word $0000, $0000          ; $FFE0-$FFE3 reserved
    .word fault                 ; $FFE4 COP (native)
    .word fault                 ; $FFE6 BRK (native)
    .word fault                 ; $FFE8 ABORT (native)
    .word nmi                   ; $FFEA NMI (native)
    .word $0000                 ; $FFEC reserved
    .word irq                   ; $FFEE IRQ (native)
    .word $0000, $0000          ; $FFF0-$FFF3 reserved
    .word fault                 ; $FFF4 COP (emulation)
    .word $0000                 ; $FFF6 reserved
    .word fault                 ; $FFF8 ABORT (emulation)
    .word nmi                   ; $FFFA NMI (emulation)
    .word reset                 ; $FFFC RESET (emulation)
    .word irq                   ; $FFFE IRQ/BRK (emulation)
    .popseg
.endmacro
