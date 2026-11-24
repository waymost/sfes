; BG mode 2 offset-per-tile (#92, ADR 0031): BG1's map has two rows of tiles, each column
; with its own palette; BG3's map, which supplies the offsets, replaces the scrolls of the
; columns after its own: BG1's tile 1 is moved 16 right and 8 down, tile 2 is left alone and
; tile 3 is moved 1016 right, which wraps. BG3 itself is not enabled. It checks nothing
; itself: a Rust test in sfes-snes-core compares the picture with values from the documents
; (ADR 0013).
; @source: anomie-regs#2105, anomie-regs#2107, anomie-regs#2109, anomie-regs#210b
; @source: anomie-regs#212c, anomie-regs#backgrounds/mode-2
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: fullsnes#snesppubgcontrol, fullsnes#snesppuvideomemoryvram
; @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
; @source: adr-0013, adr-0031

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES BG OPT MODE 2", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words: $0400
; BG1 map, $0800 BG3 map, characters from 0, 16 words to a 4-bit tile. A map entry is
; `vhopppcc cccccccc`; a BG3 entry's bits 9-0 are the offset and bit 13 applies it to BG1
; (fullsnes#snesppuvideomemoryvram). Palette entries are `ppp*16 + colour`.
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#2107, anomie-regs#2109
; @source: anomie-regs#210b, anomie-regs#212c, anomie-regs#backgrounds/mode-2
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: fullsnes#snesppuvideomemoryvram
.proc reset
    clear_vram

    ppu_write $2105, $02        ; BGMODE: mode 2, 8x8 tiles
    ppu_write $2107, $04        ; BG1SC: word $0400
    ppu_write $2109, $08        ; BG3SC: word $0800
    ppu_write $210B, $00        ; BG12NBA: BG1 characters at word 0

    vram_at $0010               ; tile 1: colour 1
    vram_rows $00FF
    vram_rows $0000
    vram_at $0020               ; tile 2: colour 2
    vram_rows $FF00
    vram_rows $0000

    vram_at $0400               ; BG1 row 0: tile 1 in palettes 0 to 3
    vram_word $0001
    vram_word $0401
    vram_word $0801
    vram_word $0C01
    vram_at $0420               ; row 1: tile 2 in palettes 0 to 3
    vram_word $0002
    vram_word $0402
    vram_word $0802
    vram_word $0C02

    vram_at $0800               ; BG3 row 0, the horizontal offsets
    vram_word $2015             ; for BG1 tile 1: 16 right ($15's low 3 bits are ignored)
    vram_word $0000             ; tile 2: none
    vram_word $23F8             ; tile 3: 1016 right
    vram_at $0820               ; BG3 row 1, the vertical offsets
    vram_word $2008             ; for BG1 tile 1: 8 down

    color 0, $7FFF              ; backdrop: white
    color 1, $001F              ; palette 0 colour 1: red
    color 33, $7C00             ; palette 2 colour 1: blue
    color 50, $03E0             ; palette 3 colour 2: green

    ppu_write $212C, $01        ; TM: BG1
    display_on
.endproc

.include "halt.inc"
