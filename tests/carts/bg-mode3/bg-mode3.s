; BG mode 3 (#91, ADR 0030): a 256-colour BG1 and a 16-colour BG2. BG1's map is 64 tiles
; wide and scrolled to 252, so its tile at map column 32, in the second screen, appears at
; pixel 4; BG2's tile is at pixel 0 and BG1's priority-0 tile is in front of BG2's. It
; checks nothing itself: a Rust test in sfes-snes-core compares the picture with values
; from the documents (ADR 0013).
; @source: anomie-regs#2105, anomie-regs#2107, anomie-regs#210b, anomie-regs#210d
; @source: anomie-regs#212c, anomie-regs#backgrounds/mode-3
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: fullsnes#snesppubgcontrol, fullsnes#snesppucolorpalettememorycgramanddirectcolors
; @source: adr-0013, adr-0030

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES BG MODE 3", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words: $0400
; BG1's first screen and $0800 its second (a 64x32 map, size bits 01), $0C00 BG2's map,
; $4000 BG1's characters (an 8-bit tile is 32 words) and $6000 BG2's (16 words). BG1's
; colour is its character data, $A5, with no palette; BG2's palette 1 starts at entry 16
; (anomie-regs#backgrounds/mode-3).
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#2107, anomie-regs#2108
; @source: anomie-regs#210b, anomie-regs#210d, anomie-regs#212c
; @source: anomie-regs#backgrounds/mode-3
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
.proc reset
    clear_vram

    ppu_write $2105, $03        ; BGMODE: mode 3, 8x8 tiles
    ppu_write $2107, $05        ; BG1SC: word $0400, 64x32
    ppu_write $2108, $0C        ; BG2SC: word $0C00, 32x32
    ppu_write $210B, $64        ; BG12NBA: BG1 at word $4000, BG2 at $6000

    ; BG1 tile 1: $A5 is planes 0, 2, 5 and 7 (anomie-regs#backgrounds/tile-maps-and-
    ; character-maps: pairs of planes, four pairs).
    vram_at $4020
    vram_rows $00FF             ; planes 0 and 1: plane 0
    vram_rows $00FF             ; planes 2 and 3: plane 2
    vram_rows $FF00             ; planes 4 and 5: plane 5
    vram_rows $FF00             ; planes 6 and 7: plane 7
    ; BG2 tile 1: colour 1.
    vram_at $6010
    vram_rows $00FF
    vram_rows $0000

    vram_at $0800               ; BG1's second screen, column 32 of row 0
    vram_word $0001             ; tile 1, palette 0, priority 0
    vram_at $0C00
    vram_word $0401             ; BG2 (0, 0): tile 1, palette 1

    ppu_write $210D, $FC        ; BG1HOFS = 252, low byte first
    ppu_write $210D, $00

    color 0, $7FFF              ; backdrop: white
    color 17, $03E0             ; BG2 palette 1 colour 1: green
    color $A5, $001F            ; BG1 colour $A5: red

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
