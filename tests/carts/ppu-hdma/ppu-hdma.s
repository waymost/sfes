; PPU registers changed mid-frame by HDMA (#100 review, ADR 0026, ADR 0028). Three HDMA
; channels write BGMODE, BG1HOFS and INIDISP in bands of 16 lines, so one frame shows BG1 in
; mode 0, scrolled, in mode 1, in forced blank, restored, and at half brightness. After the
; display is on, the CPU writes VRAM outside vertical and forced blank, which the PPU must
; refuse. It checks nothing itself: a Rust test in sfes-snes-core compares the picture with
; values from the documents (ADR 0013).
; @source: anomie-regs#2100, anomie-regs#2105, anomie-regs#210d, anomie-regs#2118
; @source: anomie-regs#dma-and-hdma/hdma, anomie-regs#backgrounds/mode-0
; @source: anomie-regs#backgrounds/mode-1, anomie-timing#s-cpu/hdma
; @source: fullsnes#snesppucontrol, adr-0013, adr-0026, adr-0028, adr-0029, adr-0030
; @source: adr-0035

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES PPU HDMA", $20, $01
cart_vectors reset, halt, halt, halt

; Reads $4212 until `branch` falls through on bit 7, vertical blank, which the load puts
; in N: bpl waits for vertical blank to begin, bmi for it to end (anomie-regs#4212).
; @source: anomie-regs#4212, wdc-w65c816s#table-5-7-1a
.macro wait_vblank branch
.local loop
loop:
    lda a:$4212                 ; HVBJOY
    branch loop
.endmacro

; hdma_channel channel, control, port, table: a direct channel reading `table` in bank $00
; (anomie-regs#43x0, #43x1, #43x2, #43x4).
; @source: anomie-regs#43x0, anomie-regs#43x1, anomie-regs#43x2, anomie-regs#43x4
.macro hdma_channel channel, control, port, table
    ppu_write $4300 + channel * $10, control
    ppu_write $4301 + channel * $10, port
    ppu_write $4302 + channel * $10, <table
    ppu_write $4303 + channel * $10, >table
    stz a:$4304 + channel * $10
.endmacro

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words:
;   $0400 BG1 map, 32x32, every entry tile 1, palette 0
;   $4000 BG1 characters. As 2-bit tiles (mode 0), tile 1 is $4008-$400F: pixels 0-3
;   colour 1. As 4-bit tiles (mode 1), tile 1 is $4010-$401F: plane 1 set, colour 2
;   (anomie-regs#backgrounds/tile-maps-and-character-maps).
; CGRAM: backdrop (16, 16, 16), colour 1 (24, 0, 0), colour 2 (0, 24, 0), whose channels
; halve exactly at brightness 7 (ADR 0035, point 2).
; HDMA writes at about H=$116 of lines 0 to 224, so a write on line L shows from line L + 1
; (anomie-regs#dma-and-hdma/hdma; ADR 0028, point 6). Each entry waits 16 lines, so band n is
; lines 16n + 1 to 16n + 16.
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#2107, anomie-regs#210b
; @source: anomie-regs#212c, anomie-regs#420c, anomie-regs#backgrounds/mode-0
; @source: anomie-regs#backgrounds/mode-1, anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: anomie-regs#dma-and-hdma/hdma, fullsnes#snesdmaandhdmanotes, adr-0026, adr-0028
.proc reset
    clear_vram

    ppu_write $2105, $00        ; BGMODE: mode 0, 8x8 tiles
    ppu_write $2107, $04        ; BG1SC: word $0400, 32x32
    ppu_write $210B, $04        ; BG12NBA: BG1 at word $4000

    vram_at $4008
    vram_rows $00F0             ; 2-bit tile 1: pixels 0-3 colour 1
    vram_rows $FF00             ; 4-bit tile 1, planes 0 and 1: colour 2

    ; The whole map: 1024 entries of tile 1, palette 0.
    vram_at $0400
    ldy #4
@rows:
    ldx #0
@entries:
    ppu_write $2118, $01
    stz a:$2119
    dex
    bne @entries
    dey
    bne @rows

    color 0, $4210              ; backdrop: (16, 16, 16)
    color 1, $0018              ; mode 0 colour 1: (24, 0, 0)
    color 2, $0300              ; mode 1 colour 2: (0, 24, 0)

    ppu_write $212C, $01        ; TM: BG1

    hdma_channel 0, $00, $05, mode_table        ; BGMODE, one register once
    hdma_channel 1, $02, $0D, scroll_table      ; BG1HOFS, one register twice
    hdma_channel 2, $00, $00, display_table     ; INIDISP, one register once

    ; Enabled in vertical blank, so the first transfers follow the setup at line 0
    ; (fullsnes#snesdmaandhdmanotes; ADR 0026, point 7).
    wait_vblank bpl
    ppu_write $420C, $07        ; HDMAEN: channels 0 to 2
    ppu_write $2100, $0F        ; INIDISP: display on, full brightness
    jmp refused_write
.endproc

; Once vertical blank ends, waits about 7 lines (256 rounds of DEX and BNE, 36 master
; cycles each from slow ROM, of 1364 a line) into band 0, which is neither vertical nor
; forced blank, and writes $00FF to word $400B, row 3 of the 2-bit tile 1. The write is
; refused: VRAM changes only in vertical or forced blank (anomie-regs#2118; ADR 0029,
; point 7). Accepted, it would turn row 3 of every tile to colour 1.
; @source: anomie-regs#2118, anomie-memmap#memory-map, anomie-timing#s-cpu/clocks-and-refresh
; @source: wdc-w65c816s#table-5-7-19a, wdc-w65c816s#table-5-7-20, fullsnes#snesppucontrol
; @source: adr-0029
.proc refused_write
    wait_vblank bmi
    ldx #0
@wait:
    dex
    bne @wait
    vram_at $400B
    vram_word $00FF
    stp
.endproc

.include "halt.inc"

.segment "RODATA"

; The tables, in the format of anomie-regs#dma-and-hdma/hdma: a line count of 16 with the
; repeat bit clear, one transfer's data, and $00 to end. After the last entry the registers
; keep their values to the end of the frame, from line 81.
; @source: anomie-regs#dma-and-hdma/hdma, anomie-regs#2100, anomie-regs#2105
; @source: anomie-regs#210d, adr-0013
mode_table:
    .byte 16, $00               ; lines 1-16: mode 0
    .byte 16, $00               ; lines 17-32: mode 0
    .byte 16, $01               ; lines 33-48: mode 1
    .byte 16, $00               ; lines 49-64: mode 0
    .byte 16, $00               ; lines 65-80: mode 0
    .byte 16, $00               ; lines 81-224: mode 0
    .byte $00
; Low byte, then high: BG1HOFS = (high << 8) | (low & ~7) | (low & 7), the low byte's
; bits 2-0 carried in from the first write (anomie-regs#210f).
scroll_table:
    .byte 16, $00, $00          ; 0
    .byte 16, $02, $00          ; 2
    .byte 16, $00, $00          ; 0
    .byte 16, $00, $00
    .byte 16, $00, $00
    .byte 16, $00, $00
    .byte $00
display_table:
    .byte 16, $0F               ; full brightness
    .byte 16, $0F
    .byte 16, $0F
    .byte 16, $80               ; forced blank
    .byte 16, $0F               ; full brightness again
    .byte 16, $07               ; brightness 7
    .byte $00
