; HDMA (#86, ADR 0026). The cartridge enables a direct and an indirect HDMA channel, both
; writing the WRAM port at $2180, in vertical blank, lets the setup at the next line 0 and
; the tables run, then copies the channel registers to WRAM. It checks nothing itself: a
; Rust test in sfes-snes-core compares the WRAM it leaves with values from the documents
; (ADR 0013).
; @source: anomie-regs#420c, anomie-regs#43x0, anomie-regs#43x5, anomie-regs#43x8
; @source: anomie-regs#43xa, anomie-regs#dma-and-hdma/hdma, anomie-regs#2180
; @source: anomie-regs#4212, anomie-timing#s-cpu/hdma
; @source: fullsnes#snesdmaandhdmachannel07registers, fullsnes#snestiminghvevents
; @source: fullsnes#snesdmaandhdmanotes, adr-0026, adr-0013

.p816
.include "header.inc"

cart_header "SFES HDMA TEST", $20, $01
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

.segment "CODE"

; Entered from the emulation-mode reset vector with D = $0000, DBR = $00 and M = X = 1
; (wdc-w65c816s#2.25), so every access is 8-bit and absolute addresses are in bank $00,
; whose $0000-$1FFF is WRAM (anomie-memmap#memory-map). WRAM it leaves:
;   $0200-$0204  $D1 $E1 $D2 $D3 $E2: by line, channel 0 then channel 1
;                (anomie-regs#dma-and-hdma/hdma; ADR 0026, points 5 and 6)
;   $0130-$0132  channel 0's $4308-$430A: direct_table + 6, then the $00 that ended it
;   $0133-$0137  channel 1's $4315, $4316, $4318-$431A: the last-channel oddity's $00
;                and $77, indirect_table + 8, and $00 (ADR 0026, point 6)
;   $0140-$0143  the addresses of direct_table and indirect_table, for the Rust test
; @source: wdc-w65c816s#2.25, anomie-memmap#memory-map, anomie-regs#2181
; @source: anomie-regs#43x0, anomie-regs#43x5, anomie-regs#43x8, anomie-regs#43xa
; @source: anomie-regs#420c, anomie-regs#dma-and-hdma/hdma, anomie-regs#4212
; @source: fullsnes#snesdmaandhdmanotes, adr-0026
.proc reset
    ; The WRAM port at $00:0200.
    stz a:$2181                 ; WMADDL
    lda #$02
    sta a:$2182                 ; WMADDM
    stz a:$2183                 ; WMADDH

    ; Channel 0: direct, mode 0, to $2180, the table in bank $00.
    stz a:$4300                 ; DMAP0
    lda #$80
    sta a:$4301                 ; BBAD0
    lda #<direct_table
    sta a:$4302                 ; A1T0L
    lda #>direct_table
    sta a:$4303                 ; A1T0H
    stz a:$4304                 ; A1B0

    ; Channel 1: indirect, mode 0, to $2180, the table and the data in bank $00.
    lda #$40
    sta a:$4310                 ; DMAP1
    lda #$80
    sta a:$4311                 ; BBAD1
    lda #<indirect_table
    sta a:$4312                 ; A1T1L
    lda #>indirect_table
    sta a:$4313                 ; A1T1H
    stz a:$4314                 ; A1B1
    stz a:$4317                 ; DASB1

    ; Enabled in vertical blank, so no line runs from the registers' power-on $FF before
    ; the setup at the next line 0 (fullsnes#snesdmaandhdmanotes; ADR 0026, point 7). The
    ; tables are done long before the next vertical blank.
    wait_vblank bpl
    lda #$03
    sta a:$420C                 ; HDMAEN
    wait_vblank bmi
    wait_vblank bpl

    ; The registers the tables left.
    ldx #0
@copy0:
    lda a:$4308,x               ; A2A0L, A2A0H, NLTR0
    sta a:$0130,x
    inx
    cpx #3
    bne @copy0
    lda a:$4315                 ; DAS1L
    sta a:$0133
    lda a:$4316                 ; DAS1H
    sta a:$0134
    ldx #0
@copy1:
    lda a:$4318,x               ; A2A1L, A2A1H, NLTR1
    sta a:$0135,x
    inx
    cpx #3
    bne @copy1
    lda #<direct_table
    sta a:$0140
    lda #>direct_table
    sta a:$0141
    lda #<indirect_table
    sta a:$0142
    lda #>indirect_table
    sta a:$0143
    stp
.endproc

; Every other vector stops the CPU too, so an unexpected interrupt is visible as a stop.
; @source: wdc-w65c816s#7.14
.proc halt
    stp
.endproc

.segment "RODATA"

; Marker bytes chosen by the cartridge (ADR 0013), in the table format of
; fullsnes#snesdmaandhdmachannel07registers.
; Direct: one unit, then a line without; then two units in repeat; then the end.
; @source: adr-0013, fullsnes#snesdmaandhdmachannel07registers
direct_table:
    .byte $02, $D1
    .byte $82, $D2, $D3
    .byte $00
; Indirect: one unit from data_a, then two lines without; one unit from data_b; then the
; end, followed by the one byte the last channel's oddity reads.
indirect_table:
    .byte $03
    .word data_a
    .byte $81
    .word data_b
    .byte $00, $77
data_a:
    .byte $E1
data_b:
    .byte $E2
