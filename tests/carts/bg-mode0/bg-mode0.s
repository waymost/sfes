; BG mode 0 (#91, ADR 0030): four 2-bit backgrounds. Each has one tile with one row of
; pixels repeated down its eight rows, and the palettes start 32 entries apart. BG1 and
; BG2 overlap with BG2's tiles at priority 1 in front of BG1's at priority 0; BG3 at
; priority 1 overlaps BG2's right half; BG4 sits one tile to the right. It checks nothing
; itself: a Rust test in sfes-snes-core compares the picture with values from the
; documents (ADR 0013).
; @source: anomie-regs#2105, anomie-regs#2107, anomie-regs#210b, anomie-regs#212c
; @source: anomie-regs#backgrounds/mode-0, anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: fullsnes#snesppubgcontrol, fullsnes#snesppucolorpalettememorycgramanddirectcolors
; @source: adr-0013, adr-0030

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES BG MODE 0", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words:
;   $0400 BG1 map, $0800 BG2 map, $0C00 BG3 map, $1000 BG4 map
;   $4000 BG1 characters, $5000 BG2, $6000 BG3, $7000 BG4 (tile 1 is 8 words in)
; Palettes: BG1 palette 0 colour 1 is entry 1; BG2 palette 1 colour 2 is 32 + 4 + 2; BG3
; palette 2 colour 3 is 64 + 8 + 3; BG4 palette 3 colour 1 is 96 + 12 + 1
; (anomie-regs#backgrounds/mode-0).
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#2107, anomie-regs#2108
; @source: anomie-regs#2109, anomie-regs#210a, anomie-regs#210b, anomie-regs#210c
; @source: anomie-regs#212c, anomie-regs#backgrounds/mode-0
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
.proc reset
    clear_vram

    ppu_write $2105, $00        ; BGMODE: mode 0, 8x8 tiles
    ppu_write $2107, $04        ; BG1SC: word $0400, 32x32
    ppu_write $2108, $08        ; BG2SC
    ppu_write $2109, $0C        ; BG3SC
    ppu_write $210A, $10        ; BG4SC
    ppu_write $210B, $54        ; BG12NBA: BG1 at word $4000, BG2 at $5000
    ppu_write $210C, $76        ; BG34NBA: BG3 at $6000, BG4 at $7000

    ; Tiles: two planes, a word a row (plane 1 in the high byte).
    vram_at $4008
    vram_rows $00F0             ; BG1: pixels 0-3 are colour 1
    vram_at $5008
    vram_rows $3C00             ; BG2: pixels 2-5 are colour 2
    vram_at $6008
    vram_rows $0F0F             ; BG3: pixels 4-7 are colour 3
    vram_at $7008
    vram_rows $00FF             ; BG4: every pixel is colour 1

    ; Map entries, vhopppcc cccccccc: tile 1.
    vram_at $0400
    vram_word $0001             ; BG1 (0, 0): palette 0, priority 0
    vram_at $0800
    vram_word $2401             ; BG2 (0, 0): palette 1, priority 1
    vram_at $0C00
    vram_word $2801             ; BG3 (0, 0): palette 2, priority 1
    vram_at $1001
    vram_word $0C01             ; BG4 (1, 0): palette 3, priority 0

    color 0, $7C00              ; backdrop: blue
    color 1, $001F              ; BG1: red
    color 38, $03E0             ; BG2: green
    color 75, $7FFF             ; BG3: white
    color 109, $7C1F            ; BG4: magenta

    ppu_write $212C, $0F        ; TM: BG1 to BG4 on the main screen
    display_on
.endproc

.include "halt.inc"
