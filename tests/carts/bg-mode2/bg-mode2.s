; BG mode 2 (#91, ADR 0030): two 4-bit backgrounds, BG1 with 16x16 tiles. BG1's map has
; one tile three times: plain, flipped horizontally and flipped vertically, so the four
; 8x8 tiles it is made of show in each of their places. BG3's map, the offset-per-tile
; data, is left empty, so no valid bit is set and no offset applies (#92 adds them). It
; checks nothing itself: a Rust test in sfes-snes-core compares the picture with values
; from the documents (ADR 0013).
; @source: anomie-regs#2105, anomie-regs#2107, anomie-regs#210b, anomie-regs#212c
; @source: anomie-regs#backgrounds/mode-2, anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: fullsnes#snesppubgcontrol, fullsnes#snesppucolorpalettememorycgramanddirectcolors
; @source: adr-0013, adr-0030

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES BG MODE 2", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words: $0400
; BG1 map, $4000 BG1 characters. A 16x16 tile 4 is the 8x8 tiles 4, 5, $14 and $15, each
; 16 words, and palette 3's colours start at entry 48
; (anomie-regs#backgrounds/tile-maps-and-character-maps).
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#2107, anomie-regs#210b
; @source: anomie-regs#212c, anomie-regs#backgrounds/mode-2
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
.proc reset
    clear_vram

    ppu_write $2105, $12        ; BGMODE: mode 2, BG1 16x16 tiles
    ppu_write $2107, $04        ; BG1SC: word $0400
    ppu_write $210B, $04        ; BG12NBA: BG1 at word $4000

    vram_at $4040               ; tile 4: colour 1
    vram_rows $00FF
    vram_rows $0000
    vram_rows $FF00             ; tile 5, next: colour 2
    vram_rows $0000
    vram_at $4140               ; tile $14: colour 3
    vram_rows $FFFF
    vram_rows $0000
    vram_rows $0000             ; tile $15, next: colour 4, which is plane 2
    vram_rows $00FF

    vram_at $0400
    vram_word $0C04             ; (0, 0): tile 4, palette 3
    vram_word $4C04             ; (1, 0): flipped horizontally
    vram_word $8C04             ; (2, 0): flipped vertically

    color 0, $7FFF              ; backdrop: white
    color 49, $001F             ; palette 3 colour 1: red
    color 50, $03E0             ; colour 2: green
    color 51, $7C00             ; colour 3: blue
    color 52, $03FF             ; colour 4: yellow

    ppu_write $212C, $01        ; TM: BG1
    display_on
.endproc

.include "halt.inc"
