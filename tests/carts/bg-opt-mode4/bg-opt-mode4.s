; BG mode 4 offset-per-tile (#92, ADR 0031): BG2's map has two rows of 2-bit tiles, each
; column with its own palette. BG3's entries supply one offset each: BG2's tile 1 gets a
; horizontal one (bit 15 clear), tile 2 a vertical one (bit 15 set), and tile 3 a
; horizontal one past the end of the map's content. It checks nothing itself: a Rust test
; in sfes-snes-core compares the picture with values from the documents (ADR 0013).
; @source: anomie-regs#2105, anomie-regs#2108, anomie-regs#2109, anomie-regs#210b
; @source: anomie-regs#212c, anomie-regs#backgrounds/mode-4
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: fullsnes#snesppubgcontrol, fullsnes#snesppuvideomemoryvram
; @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
; @source: adr-0013, adr-0031

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES BG OPT MODE 4", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words: $0400
; BG2 map, $0800 BG3 map, characters from 0, 8 words to a 2-bit tile. A BG3 entry's bits
; 9-0 are the offset, bit 14 applies it to BG2 and bit 15 makes it vertical in mode 4
; (fullsnes#snesppuvideomemoryvram). Palette entries are `ppp*4 + colour`.
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#2108, anomie-regs#2109
; @source: anomie-regs#210b, anomie-regs#212c, anomie-regs#backgrounds/mode-4
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: fullsnes#snesppuvideomemoryvram
.proc reset
    clear_vram

    ppu_write $2105, $04        ; BGMODE: mode 4, 8x8 tiles
    ppu_write $2108, $04        ; BG2SC: word $0400
    ppu_write $2109, $08        ; BG3SC: word $0800
    ppu_write $210B, $00        ; BG12NBA: BG2 characters at word 0

    vram_at $0008               ; tile 1: colour 1
    vram_rows $00FF
    vram_at $0010               ; tile 2: colour 2
    vram_rows $FF00

    vram_at $0400               ; BG2 row 0: tile 1 in palettes 0 to 3
    vram_word $0001
    vram_word $0401
    vram_word $0801
    vram_word $0C01
    vram_at $0420               ; row 1: tile 2 in palettes 0 to 3
    vram_word $0002
    vram_word $0402
    vram_word $0802
    vram_word $0C02

    vram_at $0800               ; BG3 row 0, one offset a column
    vram_word $4010             ; for BG2 tile 1: horizontal, 16 right
    vram_word $C008             ; tile 2: bit 15, vertical, 8 down
    vram_word $4008             ; tile 3: horizontal, 8 right

    color 0, $7FFF              ; backdrop: white
    color 1, $001F              ; palette 0 colour 1: red
    color 13, $03E0             ; palette 3 colour 1: green
    color 10, $7C00             ; palette 2 colour 2: blue

    ppu_write $212C, $02        ; TM: BG2
    display_on
.endproc

.include "halt.inc"
