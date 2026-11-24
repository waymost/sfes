; Sprite limits and odd sizes (#94 review, ADR 0033). All sprites are 16x32 (OBSEL size 6),
; and `$2103`'s priority bit makes sprite 2 the first. Four groups of lines show: the
; rotated priority, a 16x32 sprite flipped vertically as two square halves, time over
; dropping the first sprite's tiles, and a sprite at X -256 that uses time but draws
; nothing. It checks nothing itself: a Rust test in sfes-snes-core compares the picture and
; reads $213E with values from the documents (ADR 0013).
; @source: anomie-regs#2101, anomie-regs#2102, anomie-regs#2103, anomie-regs#213e
; @source: anomie-regs#sprites, anomie-regs#sprites/oam, anomie-regs#sprites/sprite-priority
; @source: anomie-regs#sprites/drawing-the-sprites, fullsnes#snesppuspritesobjs
; @source: adr-0013, adr-0029, adr-0033

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES OBJ LIMITS", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25).
; - OBSEL $C0: small sprites 16x32, the first table at word 0 (anomie-regs#2101). A 16x32
;   sprite of tile t is tiles t, t + 1 across and t + 16n down (ADR 0033, point 4).
; - Tiles 0 and 1, 16 and 17, 32 and 33, 48 and 49 are colours 1, 2, 3 and 4, a colour per
;   tile row; tiles 2 and 3 and those 16, 32 and 48 below them are colour 5.
; - The sprites, by index (X, Y, tile, palette; a sprite at Y is on lines Y + 1 to Y + 32):
;     1: 0, 0, 2, palette 1         2: 8, 0, 2, palette 2
;     3: 0, 40, 0, palette 0        4: 32, 40, 0, palette 0, flipped vertically
;     5: 0, 80, 2, palette 2        6 to 22: 200, 80, 2, palette 1
;     23: 32, 120, 2, palette 2     24 to 39: 200, 120, 2, palette 1
;     40: -256, 120, 2, palette 2   (X bit 8 in the high table's byte 10, bit 0)
;   The other 88 are at Y 224: with 32 rows, on no line from 1 to 224 (ADR 0033, point 3).
; - The reload address is word 4, byte 8, with the priority bit: sprite 2 is the first,
;   from the reload at the end of forced blank and at each vertical blank (ADR 0029,
;   point 6; ADR 0033, point 6).
; @source: wdc-w65c816s#2.25, anomie-regs#2101, anomie-regs#2102, anomie-regs#2103
; @source: anomie-regs#2104, anomie-regs#212c, anomie-regs#sprites, anomie-regs#sprites/oam
; @source: anomie-regs#sprites/sprite-priority, adr-0029, adr-0033
.proc reset
    clear_vram

    ppu_write $2105, $00        ; BGMODE: mode 0
    ppu_write $2101, $C0        ; OBSEL: 16x32 and 32x64, base 0

    vram_at $0000
    solid_tile 1                ; tile 0
    solid_tile 1                ; tile 1
    solid_tile 5                ; tile 2
    solid_tile 5                ; tile 3
    vram_at $0100
    solid_tile 2                ; tile 16
    solid_tile 2
    solid_tile 5
    solid_tile 5
    vram_at $0200
    solid_tile 3                ; tile 32
    solid_tile 3
    solid_tile 5
    solid_tile 5
    vram_at $0300
    solid_tile 4                ; tile 48
    solid_tile 4
    solid_tile 5
    solid_tile 5

    color 0, $7FFF              ; backdrop: white
    color 129, $001F            ; palette 0 colour 1: red
    color 130, $03E0            ; colour 2: green
    color 131, $7C00            ; colour 3: blue
    color 132, $03FF            ; colour 4: yellow
    color 149, $7FE0            ; palette 1 colour 5: cyan
    color 165, $7C1F            ; palette 2 colour 5: magenta

    ppu_write $2102, $00        ; OAMADDL
    ppu_write $2103, $00        ; OAMADDH: the low table
    sprite 0, 224, 0, $00       ; sprite 0: off the lines
    sprite 0, 0, 2, $02         ; sprite 1
    sprite 8, 0, 2, $04         ; sprite 2
    sprite 0, 40, 0, $00        ; sprite 3
    sprite 32, 40, 0, $80       ; sprite 4: flipped vertically
    sprite 0, 80, 2, $04        ; sprite 5
    .repeat 17
        sprite 200, 80, 2, $02  ; sprites 6 to 22
    .endrepeat
    sprite 32, 120, 2, $04      ; sprite 23
    .repeat 16
        sprite 200, 120, 2, $02 ; sprites 24 to 39
    .endrepeat
    sprite 0, 120, 2, $04       ; sprite 40: X bit 8 below
    .repeat 87
        sprite 0, 224, 0, $00   ; sprites 41 to 127: off the lines
    .endrepeat
    .repeat 10
        ppu_write $2104, $00    ; high table: sprites 0 to 39
    .endrepeat
    ppu_write $2104, $01        ; sprite 40: X bit 8, so X = -256
    .repeat 21
        ppu_write $2104, $00
    .endrepeat

    ppu_write $2102, $04        ; OAMADDL: word 4, sprite 2
    ppu_write $2103, $80        ; OAMADDH: the priority bit
    ppu_write $212C, $10        ; TM: the sprites
    display_on
.endproc

.include "halt.inc"
