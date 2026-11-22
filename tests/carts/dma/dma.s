; General DMA (#85, ADR 0025). The cartridge runs four transfers through the WRAM port at
; $2180: ROM to WRAM in mode 0, a fixed-address fill and a decrementing copy on two channels
; at once, $2180 to WRAM (which fails, WRAM through both buses), and $4017 through the A
; bus. It then copies channel registers to WRAM. It checks nothing itself: a Rust test in
; sfes-snes-core compares the WRAM it leaves with values from the documents (ADR 0013).
; @source: anomie-regs#420b, anomie-regs#43x0, anomie-regs#43x1, anomie-regs#43x2
; @source: anomie-regs#43x5, anomie-regs#43xb, anomie-regs#dma-and-hdma/dma
; @source: anomie-regs#2180, anomie-regs#2181, anomie-timing#s-cpu/dma
; @source: fullsnes#snesdmaandhdmachannel07registers, fullsnes#snesunpredictablethings
; @source: adr-0013, adr-0025

.p816
.include "header.inc"

cart_header "SFES DMA TEST", $20, $01
cart_vectors reset, halt, halt, halt

; Sets channel `ch`'s $43x0-$43x6: control, B-bus register, a bank-$00 A-bus address and
; the byte count (anomie-regs#43x0 to #43x5). Every access is 8-bit.
; @source: anomie-regs#43x0, anomie-regs#43x1, anomie-regs#43x2, anomie-regs#43x5
.macro channel ch, control, port, address, count
    lda #control
    sta a:$4300 + ch * $10      ; DMAPx
    lda #port
    sta a:$4301 + ch * $10      ; BBADx
    lda #<(address)
    sta a:$4302 + ch * $10      ; A1TxL
    lda #>(address)
    sta a:$4303 + ch * $10      ; A1TxH
    stz a:$4304 + ch * $10      ; A1Bx: bank $00
    lda #count
    sta a:$4305 + ch * $10      ; DASxL
    stz a:$4306 + ch * $10      ; DASxH
.endmacro

; Starts the channels in `mask`. The NOP's opcode fetch is the one more cycle the CPU gets
; before the pause, so $EA is the last value on the data bus when the transfer starts
; (anomie-timing#s-cpu/dma; ADR 0025, points 3 and 8).
; @source: anomie-regs#420b, anomie-timing#s-cpu/dma, adr-0025
.macro start mask
    lda #mask
    sta a:$420B                 ; MDMAEN
    nop
.endmacro

.segment "CODE"

; Entered from the emulation-mode reset vector with D = $0000, DBR = $00 and M = X = 1
; (wdc-w65c816s#2.25), so every access is 8-bit and absolute addresses are in bank $00,
; whose $0000-$1FFF is WRAM (anomie-memmap#memory-map). WRAM it leaves:
;   $0100-$0107  source, by channel 0 in mode 0 (anomie-regs#43x0)
;   $0108-$010C  $A5 five times, channel 2 with a fixed address (anomie-regs#43x0)
;   $010D-$0110  source's last four bytes backwards, channel 3 decrementing
;   $0111        $4017 through the A bus: open bus $EA with bits 4-2 set, $FE
;                (fullsnes#snesunpredictablethings; ADR 0025, point 6)
;   $0120-$0121  $2180 to WRAM: WRAM takes open bus, $EA, and the port does not move
;                (anomie-regs#2180; ADR 0025, point 7)
;   $0130-$0134  channel 0's $4302-$4306 after its transfer: source + 8, bank 0, count 0
;                (anomie-regs#43x2, anomie-regs#43x5)
;   $0135-$0136  channel 3's $4332-$4333: source + 3 (anomie-regs#43x2)
;   $0137        channel 0's $430B, never written: $FF (anomie-regs#43xb)
;   $0140-$0141  the address of source, for the Rust test
; @source: wdc-w65c816s#2.25, anomie-memmap#memory-map, anomie-regs#2181
; @source: anomie-regs#43x0, anomie-regs#43x2, anomie-regs#43x5, anomie-regs#43xb
; @source: anomie-regs#2180, fullsnes#snesunpredictablethings, adr-0025
.proc reset
    ; The WRAM port at $00:0100.
    stz a:$2181                 ; WMADDL
    lda #$01
    sta a:$2182                 ; WMADDM
    stz a:$2183                 ; WMADDH

    ; ROM to $2180, eight bytes, incrementing.
    channel 0, $00, $80, source, 8
    start $01

    ; A fill from one fixed byte, then the source's last four bytes backwards.
    channel 2, $08, $80, fill, 5
    channel 3, $10, $80, source + 7, 4
    start $0C

    ; $2180 to WRAM: the B-bus side fails.
    channel 1, $80, $80, $0120, 2
    start $02

    ; $4017 through the A bus.
    channel 4, $00, $80, $4017, 1
    start $10

    ; The registers the transfers left.
    ldx #0
@copy:
    lda a:$4302,x               ; A1T0L, A1T0H, A1B0, DAS0L, DAS0H
    sta a:$0130,x
    inx
    cpx #5
    bne @copy
    lda a:$4332                 ; A1T3L
    sta a:$0135
    lda a:$4333                 ; A1T3H
    sta a:$0136
    lda a:$430B                 ; UNUSED0
    sta a:$0137
    lda #<source
    sta a:$0140
    lda #>source
    sta a:$0141
    stp
.endproc

; Every other vector stops the CPU too, so an unexpected interrupt is visible as a stop.
; @source: wdc-w65c816s#7.14
.proc halt
    stp
.endproc

.segment "RODATA"

; Eight marker bytes and one fill byte, chosen by the cartridge (ADR 0013).
; @source: adr-0013
source:
    .byte $11, $22, $33, $44, $55, $66, $77, $88
fill:
    .byte $A5
