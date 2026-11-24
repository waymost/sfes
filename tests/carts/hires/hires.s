; Hi-res (#97, ADR 0036): BG mode 5 draws BG1 as 16-half-pixel tiles, the sub screen showing
; the even half-pixels and the main screen the odd, both screens showing BG1, so the picture
; is 512 wide; colour math adds the fixed colour to BG1 only, and the sub pixels take the math
; of the main pixel to their left. It checks nothing itself: a Rust test in sfes-snes-core
; compares the picture with values from the documents (ADR 0013).
; @source: anomie-regs#2100, anomie-regs#2105, anomie-regs#2107, anomie-regs#210b
; @source: anomie-regs#212c, anomie-regs#212d, anomie-regs#2130, anomie-regs#2131
; @source: anomie-regs#2132, anomie-regs#backgrounds/mode-5
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: anomie-regs#rendering-the-screen/color-math
; @source: fullsnes#snesppucolormath, fullsnes#snesppuresolution
; @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
; @source: adr-0013, adr-0036

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES HIRES", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words:
;   $0400 BG1 map, $4000 BG1 characters (a 16-colour tile is 16 words)
; BG1's map has tile 4 in its column 0 (pixels 0 to 7); every other entry is tile 0, whose
; two characters, 0 and 1, are clear. Tile 4 is two characters, 4 on the left and 5 on the
; right (anomie-regs#backgrounds/mode-5):
; every row of the left one has plane 0 `$AA`, colour 1 in its even columns, and of the
; right one plane 1 `$55`, colour 2 in its odd columns. Row r of the 16 columns is then
; "1.1.1.1." and ".2.2.2.2".
; The colours are 15-bit, `0bbbbbgggggrrrrr`: backdrop (10, 10, 10) is $294A, colour 1
; (20, 4, 0) is $0094 and colour 2 (3, 6, 9) is $24C3; the fixed colour is (5, 0, 3).
; `$2131` is `$01`: add, no half, BG1 only.
; @source: wdc-w65c816s#2.25, anomie-regs#2100, anomie-regs#2105, anomie-regs#2107
; @source: anomie-regs#210b, anomie-regs#212c, anomie-regs#212d, anomie-regs#2130
; @source: anomie-regs#2131, anomie-regs#2132, anomie-regs#backgrounds/mode-5
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: anomie-regs#rendering-the-screen/color-math
.proc reset
    clear_vram

    ppu_write $2105, $05        ; BGMODE: mode 5, 8x8 tiles
    ppu_write $2107, $04        ; BG1SC: word $0400, 32x32
    ppu_write $210B, $04        ; BG12NBA: BG1 at word $4000

    vram_at $4040               ; character 4, the left one: planes 0 and 1, then 2 and 3
    vram_rows $00AA
    vram_rows $0000
    vram_at $4050               ; character 5, the right one
    vram_rows $5500
    vram_rows $0000

    vram_at $0400
    vram_word $0004             ; BG1 column 0: tile 4, palette 0

    color 0, $294A              ; backdrop: (10, 10, 10)
    color 1, $0094              ; BG1 colour 1: (20, 4, 0)
    color 2, $24C3              ; BG1 colour 2: (3, 6, 9)

    ppu_write $2130, $00        ; CGWSEL: add the fixed colour
    ppu_write $2131, $01        ; CGADSUB: add, BG1
    ppu_write $2132, $25        ; COLDATA: red 5
    ppu_write $2132, $83        ; COLDATA: blue 3

    ppu_write $212C, $01        ; TM: BG1 on the main screen
    ppu_write $212D, $01        ; TS: and on the sub screen, as mode 5 wants
    display_on
.endproc

.include "halt.inc"
