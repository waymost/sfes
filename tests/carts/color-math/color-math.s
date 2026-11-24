; Colour math, the sub screen, the colour window and mosaic (#96, ADR 0035): BG1 on the main
; screen takes half-math with BG2 from the sub screen or the fixed colour, one tile of it is
; clipped to black by the colour window, and BG3 is pixelated in 4x4 blocks. It checks
; nothing itself: a Rust test in sfes-snes-core compares the picture with values from the
; documents (ADR 0013).
; @source: anomie-regs#2100, anomie-regs#2105, anomie-regs#2106, anomie-regs#2107
; @source: anomie-regs#2108, anomie-regs#2109, anomie-regs#210b, anomie-regs#210c
; @source: anomie-regs#212c, anomie-regs#212d, anomie-regs#2125, anomie-regs#2126
; @source: anomie-regs#2127, anomie-regs#2130, anomie-regs#2131, anomie-regs#2132
; @source: anomie-regs#backgrounds/mode-0
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: anomie-regs#rendering-the-screen/color-math
; @source: anomie-regs#rendering-the-screen/mosaic
; @source: anomie-regs#windows, fullsnes#snesppucolormath
; @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
; @source: adr-0013, adr-0035

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES COLOR MATH", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words:
;   $0400 BG1 map, $0800 BG2 map, $0C00 BG3 map
;   $4000 BG1 characters, $5000 BG2, $6000 BG3 (tile 1 is 8 words in)
; BG1's and BG2's tile 1 is colour 1 in every pixel (plane 0 set, plane 1 clear); BG3's has
; only its leftmost pixel set. Palette 0: BG1's colour 1 is CGRAM entry 1, BG2's is 32 + 1
; and BG3's is 64 + 1 (anomie-regs#backgrounds/mode-0). BG1 fills map columns 0 to 4
; (pixels 0 to 39), BG2 columns 1 and 2, BG3 columns 5 to 7.
; The colours are 15-bit, `0bbbbbgggggrrrrr`: backdrop (10, 10, 10) is $294A, BG1 (20, 4, 0)
; is $0094, BG2 (3, 6, 9) is $24C3, BG3 white, and the fixed colour is (5, 0, 3).
; The colour window is window 1, pixels 24 to 31 (anomie-regs#2125: window 1 enabled in the
; colour window's high nibble, `$2`). `$2130` is `$82`: clip colours inside the colour
; window (bits 7 and 6 = `10`) and add the sub screen (bit 1). `$2131` is `$41`: add, half,
; on BG1 only. `$2106` is `$34`: 4x4 blocks (size field 3) on BG3 (bit 2).
; @source: wdc-w65c816s#2.25, anomie-regs#2100, anomie-regs#2105, anomie-regs#2106
; @source: anomie-regs#2107, anomie-regs#2108, anomie-regs#2109, anomie-regs#210b
; @source: anomie-regs#210c, anomie-regs#212c, anomie-regs#212d, anomie-regs#2125
; @source: anomie-regs#2126, anomie-regs#2127, anomie-regs#2130, anomie-regs#2131
; @source: anomie-regs#2132, anomie-regs#backgrounds/mode-0
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: anomie-regs#rendering-the-screen/color-math
; @source: anomie-regs#rendering-the-screen/mosaic, anomie-regs#windows
.proc reset
    clear_vram

    ppu_write $2105, $00        ; BGMODE: mode 0, 8x8 tiles
    ppu_write $2107, $04        ; BG1SC: word $0400, 32x32
    ppu_write $2108, $08        ; BG2SC
    ppu_write $2109, $0C        ; BG3SC
    ppu_write $210B, $54        ; BG12NBA: BG1 at word $4000, BG2 at $5000
    ppu_write $210C, $06        ; BG34NBA: BG3 at $6000

    vram_at $4008
    vram_rows $00FF             ; BG1 tile 1: colour 1
    vram_at $5008
    vram_rows $00FF             ; BG2
    vram_at $6008
    vram_rows $0080             ; BG3: the leftmost pixel only

    vram_at $0400
    .repeat 5
        vram_word $0001         ; BG1 columns 0 to 4: tile 1, palette 0
    .endrepeat
    vram_at $0801
    .repeat 2
        vram_word $0001         ; BG2 columns 1 and 2
    .endrepeat
    vram_at $0C05
    .repeat 3
        vram_word $0001         ; BG3 columns 5 to 7
    .endrepeat

    color 0, $294A              ; backdrop: (10, 10, 10)
    color 1, $0094              ; BG1: (20, 4, 0)
    color 33, $24C3             ; BG2: (3, 6, 9)
    color 65, $7FFF             ; BG3: white

    ppu_write $2126, 24         ; WH0: window 1 left
    ppu_write $2127, 31         ; WH1: window 1 right
    ppu_write $2125, $20        ; WOBJSEL: the colour window uses window 1
    ppu_write $2130, $82        ; CGWSEL: clip inside the window, add the sub screen
    ppu_write $2131, $41        ; CGADSUB: add, half, BG1
    ppu_write $2132, $25        ; COLDATA: red 5
    ppu_write $2132, $83        ; COLDATA: blue 3
    ppu_write $2106, $34        ; MOSAIC: 4x4 blocks on BG3

    ppu_write $212C, $05        ; TM: BG1 and BG3 on the main screen
    ppu_write $212D, $02        ; TS: BG2 on the sub screen
    display_on
.endproc

; Every other vector stops the CPU too, so an unexpected interrupt is visible as a stop.
; @source: wdc-w65c816s#7.14
.proc halt
    stp
.endproc

.segment "RODATA"

; One zero byte for the fixed-address VRAM clear (ADR 0013).
; @source: adr-0013
zero:
    .byte $00
