; Sprites (#94, ADR 0033): two overlapping 8x8 sprites, where sprite 0 is in front of sprite
; 1, and one 16x16 sprite flipped both ways, each tile a solid colour; and 33 more sprites
; on line 101, which is more than 32 and sets the range-over flag of $213E. The sprites are
; the only layer on the main screen. It checks nothing itself: a Rust test in sfes-snes-core
; compares the picture and reads $213E with values from the documents (ADR 0013).
; @source: anomie-regs#2101, anomie-regs#2102, anomie-regs#2103, anomie-regs#2104
; @source: anomie-regs#212c, anomie-regs#213e, anomie-regs#sprites
; @source: anomie-regs#sprites/oam, anomie-regs#sprites/palettes
; @source: anomie-regs#sprites/sprite-priority, anomie-regs#sprites/drawing-the-sprites
; @source: fullsnes#snesppuspritesobjs, fullsnes#snespputimersandstatus
; @source: adr-0013, adr-0033

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES SPRITES", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). OBSEL is 0: 8x8 and
; 16x16 sprites, the first table at word 0 (anomie-regs#2101). Tile n starts at word n * 16:
; tile 0 is colour 1, tile 1 colour 2 and the 16x16 sprite's tiles $20, $21, $30 and $31
; (a 16x16 sprite is the tile, the next, and those 16 later, anomie-regs#sprites) are
; colours 1 to 4. Sprite 0 is 8x8 at X 8, Y 0 with palette 0; sprite 1 is 8x8 at X 12 with
; palette 1; sprite 2 is the large sprite at X 32, with both flips (vh = 11) and palette 2.
; Sprites 3 to 35 are 8x8 at Y 100 and the other 92 at Y 128, below the rows the first three
; cover. The high table is all zero except sprite 2's size bit, which is bit 1 of the
; table's first byte shifted by 2 * 2 = bit 5 (anomie-regs#sprites/oam).
; @source: wdc-w65c816s#2.25, anomie-regs#2101, anomie-regs#2102, anomie-regs#2103
; @source: anomie-regs#2104, anomie-regs#2105, anomie-regs#212c, anomie-regs#sprites
; @source: anomie-regs#sprites/oam, anomie-regs#sprites/palettes
.proc reset
    clear_vram

    ppu_write $2105, $00        ; BGMODE: mode 0
    ppu_write $2101, $00        ; OBSEL: 8x8 and 16x16, base 0

    vram_at $0000
    solid_tile 1                ; tile 0
    solid_tile 2                ; tile 1
    vram_at $0200
    solid_tile 1                ; tile $20
    solid_tile 2                ; tile $21
    vram_at $0300
    solid_tile 3                ; tile $30
    solid_tile 4                ; tile $31

    color 0, $7FFF              ; backdrop: white
    color 129, $001F            ; palette 0, colour 1: red
    color 146, $7C00            ; palette 1, colour 2: blue
    color 161, $03E0            ; palette 2, colour 1: green
    color 162, $7C1F            ; palette 2, colour 2: magenta
    color 163, $03FF            ; palette 2, colour 3: yellow
    color 164, $7FE0            ; palette 2, colour 4: cyan

    ppu_write $2102, $00        ; OAMADDL
    ppu_write $2103, $00        ; OAMADDH: the low table, no priority rotation
    sprite 8, 0, 0, $00         ; sprite 0
    sprite 12, 0, 1, $02        ; sprite 1: palette 1
    sprite 32, 0, $20, $C4      ; sprite 2: both flips, palette 2
    .repeat 33
        sprite 0, 100, 0, $00   ; sprites 3 to 35
    .endrepeat
    .repeat 92
        sprite 0, 128, 0, $00   ; sprites 36 to 127: lines 129 to 136
    .endrepeat
    ppu_write $2104, $20        ; the high table: sprite 2 is large
    .repeat 31
        ppu_write $2104, $00
    .endrepeat

    ppu_write $212C, $10        ; TM: the sprites
    display_on
.endproc

.include "halt.inc"
