; Interlace, pseudo-hi-res and mode 6 by band (#97 review, ADR 0036). Interlace is on all
; frame; five HDMA channels write BGMODE, SETINI, TM, TS and BG1SC so that lines 1 to 16 are
; mode 5, whose interlaced rows step by two and differ by field, lines 17 to 32 mode 1 in
; pseudo-hi-res with BG2 on the sub screen, and line 33 on mode 6 with an offset-per-tile
; column. It checks nothing itself: a Rust test in sfes-snes-core compares two frames, one
; of each field, with values from the documents (ADR 0013).
; @source: anomie-regs#2105, anomie-regs#2107, anomie-regs#2133, anomie-regs#212c
; @source: anomie-regs#212d, anomie-regs#backgrounds/mode-5, anomie-regs#backgrounds/mode-6
; @source: anomie-regs#backgrounds/bg-scrolling, anomie-regs#dma-and-hdma/hdma
; @source: fullsnes#snesppuresolution, adr-0013, adr-0026, adr-0028, adr-0031, adr-0036

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES HIRES BANDS", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words:
;   $0400 map A (mode 5): column 0 of each row is entry 2 on even tile rows, 6 on odd
;   $0800 map B (mode 1, BG1): column 0 of each row is entry 3
;   $0C00 map C (mode 6): columns 0 and 3 entry 6, column 1 entry 8, in every row
;   $1000 BG2's map (mode 1): column 0 of each row is entry 6
;   $1400 BG3's map: row 0, column 0 is $2010, BG1's horizontal offset 16
;   $4000 characters for BG1 and BG2, 4-bit, 16 words each
; Character 2 has colour 1 on even rows and 3 on odd; 3 is colour 2, 6 to 9 are colours 4
; to 7. In modes 5 and 6 entry n is character n on the left half of the 16 half-pixels and
; n + 1 on the right (ADR 0036, point 3), so characters 0 and 1, which every empty entry
; shows, stay clear.
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#2107, anomie-regs#2108
; @source: anomie-regs#2109, anomie-regs#210b, anomie-regs#2133
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps, adr-0026, adr-0036
.proc reset
    clear_vram

    ppu_write $2105, $05        ; BGMODE: mode 5, 8x8 tiles
    ppu_write $2133, $01        ; SETINI: interlace
    ppu_write $2107, $04        ; BG1SC: map A
    ppu_write $2108, $10        ; BG2SC: word $1000
    ppu_write $2109, $14        ; BG3SC: word $1400
    ppu_write $210B, $44        ; BG12NBA: BG1 and BG2 at word $4000

    ; Character 2: colour 1 (plane 0) on even rows, 3 (planes 0 and 1) on odd.
    vram_at $4020
    .repeat 4
        vram_word $00FF
        vram_word $FFFF
    .endrepeat
    vram_rows $0000
    solid_tile 2                ; character 3
    vram_at $4060
    solid_tile 4                ; character 6
    solid_tile 5                ; character 7
    solid_tile 6                ; character 8
    solid_tile 7                ; character 9

    .repeat 32, row
        vram_at $0400 + row * 32
        vram_word 2 + (row & 1) * 4
        vram_at $0800 + row * 32
        vram_word 3
        vram_at $0C00 + row * 32
        vram_word 6
        vram_word 8
        vram_word 0
        vram_word 6
        vram_at $1000 + row * 32
        vram_word 6
    .endrepeat
    vram_at $1400
    vram_word $2010

    color 0, $7FFF              ; backdrop: white
    color 1, $001F              ; red
    color 2, $03E0              ; green
    color 3, $7C00              ; blue
    color 4, $03FF              ; yellow
    color 5, $7C1F              ; magenta
    color 6, $7FE0              ; cyan
    color 7, $294A              ; grey

    ppu_write $212C, $01        ; TM: BG1
    ppu_write $212D, $01        ; TS: BG1

    hdma_channel 0, $00, $05, mode_table        ; BGMODE
    hdma_channel 1, $00, $33, setini_table      ; SETINI
    hdma_channel 2, $00, $2C, main_table        ; TM
    hdma_channel 3, $00, $2D, sub_table         ; TS
    hdma_channel 4, $00, $07, map_table         ; BG1SC

    ; Enabled in vertical blank, so the first transfers follow the setup at line 0
    ; (fullsnes#snesdmaandhdmanotes; ADR 0026, point 7).
    wait_vblank bpl
    ppu_write $420C, $1F        ; HDMAEN: channels 0 to 4
    display_on
.endproc

.include "halt.inc"

.segment "RODATA"

; Three bands of 16 lines (anomie-regs#dma-and-hdma/hdma; ADR 0028, point 6); the last
; band's values hold to the end of the frame. Every band keeps interlace on.
;   lines 1-16:  mode 5, BG1 on both screens, map A
;   lines 17-32: mode 1 with pseudo-hi-res ($2133 bit 3), BG1 main and BG2 sub, map B
;   lines 33 on: mode 6, BG1 on both screens, map C
; @source: anomie-regs#dma-and-hdma/hdma, anomie-regs#2105, anomie-regs#2133
; @source: anomie-regs#212c, anomie-regs#212d, anomie-regs#2107, adr-0013, adr-0028
mode_table:
    .byte 16, $05, 16, $01, 16, $06, $00
setini_table:
    .byte 16, $01, 16, $09, 16, $01, $00
main_table:
    .byte 16, $01, 16, $01, 16, $01, $00
sub_table:
    .byte 16, $01, 16, $02, 16, $01, $00
map_table:
    .byte 16, $04, 16, $08, 16, $0C, $00
