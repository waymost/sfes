; Mode 7 transforms by band (#93 review, ADR 0032). All eight HDMA channels write M7SEL, the
; matrix, the two mode 7 scrolls and TM every 16 lines, so one frame shows the identity
; matrix, the horizontal flip, the three ways to fill outside the playfield, the axes swapped
; by B and C, the vertical flip, and EXTBG's BG2 in front of BG1. The playfield is the same in
; every row: tilemap column 0 is character 1, column 1 character 2 and column 127 character
; 4. It checks nothing itself: a Rust test in sfes-snes-core compares the picture with values
; from the documents (ADR 0013).
; @source: anomie-regs#211a, anomie-regs#211b, anomie-regs#210d, anomie-regs#2133
; @source: anomie-regs#backgrounds/mode-7, anomie-regs#dma-and-hdma/hdma
; @source: fullsnes#snesppurotationscaling, adr-0013, adr-0026, adr-0028, adr-0032

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES MODE 7 BANDS", $20, $01
cart_vectors reset, halt, halt, halt

; character number, pixel: the 64 pixels of `number`, all `pixel`, into the high bytes of
; words `number * 64` on (anomie-regs#backgrounds/mode-7), with VMAIN = $80.
; @source: anomie-regs#backgrounds/mode-7, anomie-regs#2119
.macro character number, pixel
.local loop
    vram_at number * 64
    ldx #64
    lda #pixel
loop:
    sta a:$2119                 ; VMDATAH
    dex
    bne loop
.endmacro

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25).
; - The tilemap is the low byte of VRAM words 0 to 16383, 128 entries a row; a character's
;   64 pixels are the high bytes of words `character * 64` on, one byte a pixel, 0 being
;   transparent (anomie-regs#backgrounds/mode-7). With VMAIN = $00 the address steps after
;   each `$2118` write, with $80 after each `$2119` write (anomie-regs#2115), so the two
;   bytes of a word are written apart.
; - Each map row is `map_row` by DMA: columns 0, 1 and 127 are characters 1, 2 and 4, the
;   rest character 5, which stays clear. Character 0 is never on the map: it shows only as
;   the fill outside the playfield.
; - Character pixels: 0 is $06, 1 is $01, 2 is $02 and 4 is $84. As BG1 they are CGRAM
;   entries 6, 1, 2 and 132; as EXTBG's BG2, $84 is entry 4 with priority 1 and the others
;   their own entry with priority 0 (anomie-regs#backgrounds/mode-7).
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#2115, anomie-regs#2116
; @source: anomie-regs#2118, anomie-regs#2133, anomie-regs#212c, anomie-regs#43x0
; @source: anomie-regs#43x5, anomie-regs#420b, anomie-regs#420c
; @source: anomie-regs#backgrounds/mode-7, fullsnes#snesdmaandhdmanotes, adr-0026
; @source: adr-0032
.proc reset
    clear_vram

    ppu_write $2105, $07        ; BGMODE: mode 7
    ppu_write $2133, $40        ; SETINI: EXTBG

    ; The map: 128 DMAs of the 128-byte row into $2118.
    ppu_write $2115, $00        ; VMAIN: increment after $2118
    vram_at $0000
    ldy #128
@row:
    ppu_write $4300, $00        ; DMAP0: one register, A address incremented
    ppu_write $4301, $18        ; BBAD0: $2118
    ppu_write $4302, <map_row   ; A1T0L
    ppu_write $4303, >map_row   ; A1T0H
    stz a:$4304                 ; A1B0
    ppu_write $4305, 128        ; DAS0L
    stz a:$4306                 ; DAS0H
    ppu_write $420B, $01        ; MDMAEN: channel 0
    dey
    bne @row

    ; The characters, into the high bytes.
    ppu_write $2115, $80        ; VMAIN: increment after $2119
    character 0, $06
    character 1, $01
    character 2, $02
    character 4, $84

    color 0, $7FFF              ; backdrop: white
    color 1, $001F              ; character 1: red
    color 2, $7C00              ; character 2: blue
    color 4, $03FF              ; character 4 as BG2: yellow
    color 6, $7C1F              ; character 0: magenta
    color 132, $7FE0            ; character 4 as BG1: cyan

    hdma_channel 0, $00, $1A, select_table  ; M7SEL
    hdma_channel 1, $02, $1B, a_table       ; M7A, written twice
    hdma_channel 2, $02, $1C, b_table       ; M7B
    hdma_channel 3, $02, $1D, c_table       ; M7C
    hdma_channel 4, $02, $1E, d_table       ; M7D
    hdma_channel 5, $02, $0D, h_table       ; M7HOFS
    hdma_channel 6, $02, $0E, v_table       ; M7VOFS
    hdma_channel 7, $00, $2C, layers_table  ; TM

    ; Enabled in vertical blank, so the first transfers follow the setup at line 0
    ; (fullsnes#snesdmaandhdmanotes; ADR 0026, point 7).
    wait_vblank bpl
    ppu_write $420C, $FF        ; HDMAEN: channels 0 to 7
    display_on
.endproc

.include "halt.inc"

.segment "RODATA"

; One map row (anomie-regs#backgrounds/mode-7): characters 1 and 2, 125 of the clear
; character 5, and character 4 in column 127.
; @source: anomie-regs#backgrounds/mode-7, adr-0013
map_row:
    .byte 1, 2
    .res 125, 5
    .byte 4

; The bands, in the table format of anomie-regs#dma-and-hdma/hdma: 16 lines each, the
; repeat bit clear. A write on line L shows from line L + 1 (ADR 0028, point 6), so band n
; is lines 16n + 1 to 16n + 16, and the last band's values hold from line 113 to 224.
; The matrix registers are 1.0 = $0100, signed (anomie-regs#211b); the scrolls are 13-bit
; signed, -8 = $1FF8, -81 = $1FAF, -143 = $1F71 (anomie-regs#210d); each word is written low
; byte first.
;   0: identity                       4: as 2, character 0 outside ($C0)
;   1: identity, flipped across ($01) 5: A = D = 0, B = C = 1.0, VOFS -81
;   2: identity, HOFS -8, wrapping    6: as 5 flipped down ($02), VOFS -143
;   3: as 2, transparent outside ($80) 7: as 2, with BG2 on the main screen
; @source: anomie-regs#dma-and-hdma/hdma, anomie-regs#211a, anomie-regs#211b
; @source: anomie-regs#210d, anomie-regs#212c, adr-0013, adr-0028
select_table:
    .byte 16, $00, 16, $01, 16, $00, 16, $80, 16, $C0, 16, $00, 16, $02, 16, $00, $00
a_table:
    .byte 16, $00, $01, 16, $00, $01, 16, $00, $01, 16, $00, $01, 16, $00, $01
    .byte 16, $00, $00, 16, $00, $00, 16, $00, $01, $00
b_table:
    .byte 16, $00, $00, 16, $00, $00, 16, $00, $00, 16, $00, $00, 16, $00, $00
    .byte 16, $00, $01, 16, $00, $01, 16, $00, $00, $00
c_table:
    .byte 16, $00, $00, 16, $00, $00, 16, $00, $00, 16, $00, $00, 16, $00, $00
    .byte 16, $00, $01, 16, $00, $01, 16, $00, $00, $00
d_table:
    .byte 16, $00, $01, 16, $00, $01, 16, $00, $01, 16, $00, $01, 16, $00, $01
    .byte 16, $00, $00, 16, $00, $00, 16, $00, $01, $00
h_table:
    .byte 16, $00, $00, 16, $00, $00, 16, $F8, $1F, 16, $F8, $1F, 16, $F8, $1F
    .byte 16, $00, $00, 16, $00, $00, 16, $F8, $1F, $00
v_table:
    .byte 16, $00, $00, 16, $00, $00, 16, $00, $00, 16, $00, $00, 16, $00, $00
    .byte 16, $AF, $1F, 16, $71, $1F, 16, $00, $00, $00
layers_table:
    .byte 16, $01, 16, $01, 16, $01, 16, $01, 16, $01, 16, $01, 16, $01, 16, $03, $00
