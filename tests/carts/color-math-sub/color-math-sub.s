; Subtraction and the sub screen's window (#96 review, ADR 0034, ADR 0035). BG1 on the main
; screen subtracts BG2 on the sub screen, with the clamp at 0; TSW masks BG2 in window 2, so
; the fixed colour is subtracted there; the colour window prevents math in window 1; the
; backdrop takes math; and of two sprites only the one in palette 4 to 7 does. It checks
; nothing itself: a Rust test in sfes-snes-core compares the picture with values from the
; documents (ADR 0013).
; @source: anomie-regs#2123, anomie-regs#2125, anomie-regs#2126, anomie-regs#212f
; @source: anomie-regs#2130, anomie-regs#2131, anomie-regs#2132
; @source: anomie-regs#rendering-the-screen/color-math, anomie-regs#windows
; @source: fullsnes#snesppucolormath, adr-0013, adr-0033, adr-0034, adr-0035

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES MATH SUB", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words:
;   $0000 sprite tile 0, colour 1 (OBSEL 0: 8x8 sprites, the table at word 0)
;   $0400 BG1 map, $0800 BG2 map: columns 0 to 7 of row 0 are tile 1, palette 0
;   $4000 BG1 characters, $5000 BG2: tile 1 is colour 1
; The colours are 15-bit, `0bbbbbgggggrrrrr`: backdrop (10, 10, 10) is $294A, BG1 and the
; sprites (20, 20, 20) $5294, BG2 (6, 12, 24) $6186. The fixed colour is (2, 2, 2).
; Window 1 is pixels 16 to 23, window 2 pixels 32 to 47 (anomie-regs#2126).
; Sprite 0 is at X 64 in palette 0 and sprite 1 at X 80 in palette 4, both at Y 0; the
; other 126 are at Y 224, on no line (ADR 0033, point 3).
; @source: wdc-w65c816s#2.25, anomie-regs#2101, anomie-regs#2105, anomie-regs#2107
; @source: anomie-regs#210b, anomie-regs#2123, anomie-regs#2125, anomie-regs#2126
; @source: anomie-regs#212c, anomie-regs#212d, anomie-regs#212f, anomie-regs#2130
; @source: anomie-regs#2131, anomie-regs#2132, anomie-regs#sprites/oam
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps, adr-0034, adr-0035
.proc reset
    clear_vram

    ppu_write $2105, $00        ; BGMODE: mode 0, 8x8 tiles
    ppu_write $2101, $00        ; OBSEL: 8x8 and 16x16, base 0
    ppu_write $2107, $04        ; BG1SC: word $0400, 32x32
    ppu_write $2108, $08        ; BG2SC
    ppu_write $210B, $54        ; BG12NBA: BG1 at word $4000, BG2 at $5000

    vram_at $0000
    solid_tile 1                ; sprite tile 0
    vram_at $4008
    vram_rows $00FF             ; BG1 tile 1: colour 1
    vram_at $5008
    vram_rows $00FF             ; BG2

    vram_at $0400
    .repeat 8
        vram_word $0001         ; BG1 columns 0 to 7
    .endrepeat
    vram_at $0800
    .repeat 8
        vram_word $0001         ; BG2 columns 0 to 7
    .endrepeat

    color 0, $294A              ; backdrop: (10, 10, 10)
    color 1, $5294              ; BG1: (20, 20, 20)
    color 33, $6186             ; BG2: (6, 12, 24)
    color 129, $5294            ; sprite palette 0 colour 1
    color 193, $5294            ; sprite palette 4 colour 1

    ppu_write $2102, $00        ; OAMADDL
    ppu_write $2103, $00        ; OAMADDH
    sprite 64, 0, 0, $00        ; sprite 0: palette 0
    sprite 80, 0, 0, $08        ; sprite 1: palette 4
    .repeat 126
        sprite 0, 224, 0, $00   ; sprites 2 to 127: off the lines
    .endrepeat
    .repeat 32
        ppu_write $2104, $00    ; high table: small, X bit 8 clear
    .endrepeat

    ppu_write $2126, 16         ; WH0: window 1 left
    ppu_write $2127, 23         ; WH1: window 1 right
    ppu_write $2128, 32         ; WH2: window 2 left
    ppu_write $2129, 47         ; WH3: window 2 right
    ppu_write $2123, $80        ; W12SEL: BG2 uses window 2
    ppu_write $2125, $20        ; WOBJSEL: the colour window uses window 1
    ppu_write $212F, $02        ; TSW: BG2 is masked on the sub screen
    ppu_write $2130, $22        ; CGWSEL: prevent math inside, use the sub screen
    ppu_write $2131, $B1        ; CGADSUB: subtract; the backdrop, OBJ and BG1
    ppu_write $2132, $E2        ; COLDATA: red, green and blue 2

    ppu_write $212C, $11        ; TM: BG1 and the sprites
    ppu_write $212D, $02        ; TS: BG2
    display_on
.endproc

.include "halt.inc"
