; BG mode 1 (#91, ADR 0030): two 4-bit backgrounds and a 2-bit one, with `$2105` bit 3 set
; so BG3's priority-1 tiles are in front of everything. BG1 and BG3 overlap, both at
; priority 1; BG2 at priority 0 shows behind them. It checks nothing itself: a Rust test in
; sfes-snes-core compares the picture with values from the documents (ADR 0013).
; @source: anomie-regs#2105, anomie-regs#2107, anomie-regs#210b, anomie-regs#212c
; @source: anomie-regs#backgrounds/mode-1, anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: fullsnes#snesppubgcontrol, fullsnes#snesppucolorpalettememorycgramanddirectcolors
; @source: adr-0013, adr-0030

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES BG MODE 1", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words:
;   $0400 BG1 map, $0800 BG2 map, $0C00 BG3 map
;   $4000 BG1 characters, $5000 BG2, $6000 BG3; a 4-bit tile is 16 words, a 2-bit tile 8
; BG1 uses palette 2, so its colour 1 is entry 2 * 16 + 1; BG3 uses palette 1, so its
; colour 1 is entry 1 * 4 + 1 (anomie-regs#backgrounds/mode-1).
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#2107, anomie-regs#2108
; @source: anomie-regs#2109, anomie-regs#210b, anomie-regs#210c, anomie-regs#212c
; @source: anomie-regs#backgrounds/mode-1
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
.proc reset
    clear_vram

    ppu_write $2105, $09        ; BGMODE: mode 1, BG3 priority bit, 8x8 tiles
    ppu_write $2107, $04        ; BG1SC: word $0400
    ppu_write $2108, $08        ; BG2SC: word $0800
    ppu_write $2109, $0C        ; BG3SC: word $0C00
    ppu_write $210B, $54        ; BG12NBA: BG1 at word $4000, BG2 at $5000
    ppu_write $210C, $06        ; BG34NBA: BG3 at $6000

    ; BG1 tile 1: planes 0 and 1, then planes 2 and 3.
    vram_at $4010
    vram_rows $00F0             ; pixels 0-3 are colour 1
    vram_rows $0000
    ; BG2 tile 1: every pixel is colour 2.
    vram_at $5010
    vram_rows $FF00
    vram_rows $0000
    ; BG3 tile 1, two planes: pixels 2-5 are colour 1.
    vram_at $6008
    vram_rows $003C

    vram_at $0400
    vram_word $2801             ; BG1 (0, 0): tile 1, palette 2, priority 1
    vram_at $0800
    vram_word $0001             ; BG2 (0, 0): palette 0, priority 0
    vram_at $0C00
    vram_word $2401             ; BG3 (0, 0): palette 1, priority 1

    color 0, $7FFF              ; backdrop: white
    color 2, $7C00              ; BG2 colour 2: blue
    color 5, $03E0              ; BG3 palette 1 colour 1: green
    color 33, $001F             ; BG1 palette 2 colour 1: red

    ppu_write $212C, $07        ; TM: BG1 to BG3
    display_on
.endproc

.include "halt.inc"
