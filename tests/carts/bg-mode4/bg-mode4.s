; BG mode 4 (#91, ADR 0030): a 256-colour BG1 in direct colour mode and a 4-colour BG2.
; BG1's one tile is the character byte $B6 with palette bits 5, BG2's is one tile to the
; right. BG3's map, the offset-per-tile data, is left empty (#92 adds them). It checks
; nothing itself: a Rust test in sfes-snes-core compares the picture with values from the
; documents (ADR 0013).
; @source: anomie-regs#2105, anomie-regs#2107, anomie-regs#210b, anomie-regs#212c
; @source: anomie-regs#2130, anomie-regs#backgrounds/direct-color-mode
; @source: anomie-regs#backgrounds/mode-4
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: fullsnes#snesppubgcontrol, fullsnes#snesppucolorpalettememorycgramanddirectcolors
; @source: adr-0013, adr-0030

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES BG MODE 4", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words: $0400
; BG1 map, $0800 BG2 map, $4000 BG1 characters, $6000 BG2's. $B6 is BBGGGRRR = 10 110 110,
; planes 1, 2, 4, 5 and 7; palette 5 is bgr = 101. The colour is BBb00 RRRr0 GGGg0:
; blue 10 1, green 110 0, red 110 1, which is $531A (anomie-regs#backgrounds/direct-color-
; mode). BG2's palette 1 colour 3 is entry 1 * 4 + 3 (anomie-regs#backgrounds/mode-4).
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#2107, anomie-regs#2108
; @source: anomie-regs#210b, anomie-regs#212c, anomie-regs#2130
; @source: anomie-regs#backgrounds/direct-color-mode, anomie-regs#backgrounds/mode-4
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
.proc reset
    clear_vram

    ppu_write $2105, $04        ; BGMODE: mode 4, 8x8 tiles
    ppu_write $2107, $04        ; BG1SC: word $0400
    ppu_write $2108, $08        ; BG2SC: word $0800
    ppu_write $210B, $64        ; BG12NBA: BG1 at word $4000, BG2 at $6000
    ppu_write $2130, $01        ; CGWSEL: direct colour

    vram_at $4020
    vram_rows $FF00             ; planes 0 and 1: plane 1
    vram_rows $00FF             ; planes 2 and 3: plane 2
    vram_rows $FFFF             ; planes 4 and 5
    vram_rows $FF00             ; planes 6 and 7: plane 7
    vram_at $6008
    vram_rows $FFFF             ; BG2 tile 1: colour 3

    vram_at $0400
    vram_word $1401             ; BG1 (0, 0): tile 1, palette 5
    vram_at $0801
    vram_word $0401             ; BG2 (1, 0): tile 1, palette 1

    color 0, $7FFF              ; backdrop: white
    color 7, $03E0              ; BG2 palette 1 colour 3: green

    ppu_write $212C, $03        ; TM: BG1 and BG2
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
