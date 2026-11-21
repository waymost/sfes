; NMI, the H/V timer IRQ and the counter latch (#82, ADR 0021). The cartridge sets an HV-IRQ
; at H=100, V=5 and waits for it; the handler acknowledges it and latches and reads the H/V
; counters. It then enables the NMI alone and waits again; the NMI handler reads $4210 twice
; and $4212. It checks nothing itself: a Rust test in sfes-snes-core compares the interrupt
; entry times and the WRAM it leaves with values computed from the documents (ADR 0013).
; @source: anomie-timing#s-cpu/interrupts
; @source: anomie-regs#4200, anomie-regs#4207, anomie-regs#4209, anomie-regs#4210
; @source: anomie-regs#4211, anomie-regs#4212, anomie-regs#2137, anomie-regs#213c
; @source: adr-0021
; @source: adr-0013

.p816
.include "header.inc"

cart_header "SFES INTERRUPTS", $20, $01
cart_vectors reset, nmi, irq, halt

.segment "CODE"

; Entered from the emulation-mode reset vector with I set, D = $0000, DBR = $00 and
; M = X = 1 (wdc-w65c816s#2.25), so every access below is 8-bit and absolute addresses
; are in bank $00, whose $0000-$1FFF is WRAM (anomie-memmap#memory-map).
; @source: wdc-w65c816s#2.25, anomie-regs#4200, anomie-regs#4207, anomie-regs#4208
; @source: anomie-regs#4209, anomie-regs#420a, wdc-w65c816s#7.13
.proc reset
    lda #100
    sta a:$4207                 ; HTIMEL = 100
    stz a:$4208                 ; HTIMEH = 0
    lda #5
    sta a:$4209                 ; VTIMEL = 5
    stz a:$420A                 ; VTIMEH = 0
    lda #$30
    sta a:$4200                 ; NMITIMEN: HV-IRQ, NMI off
    cli
    wai                         ; the IRQ handler runs, then RTI returns here
    lda #$80
    sta a:$4200                 ; NMITIMEN: NMI on; clearing bits 4-5 also acknowledges
    wai                         ; the NMI handler runs, then RTI returns here
    stp
.endproc

; The IRQ handler: $4211 acknowledges the IRQ, then $2137 latches the counters (WRIO bit 7
; is set from power-on) and both bytes of each are read.
; @source: anomie-regs#4211, anomie-regs#2137, anomie-regs#213c, anomie-regs#213d
; @source: anomie-regs#4201, fullsnes#snespputimersandstatus
.proc irq
    lda a:$4211
    sta a:$0000                 ; TIMEUP
    lda a:$2137                 ; latch
    lda a:$213C
    sta a:$0001                 ; OPHCT low
    lda a:$213C
    sta a:$0002                 ; OPHCT high
    lda a:$213D
    sta a:$0003                 ; OPVCT low
    lda a:$213D
    sta a:$0004                 ; OPVCT high
    rti
.endproc

; The NMI handler: $4210 with the flag set, $4212 in vertical blank, and $4210 again after
; the first read cleared the flag.
; @source: anomie-regs#4210, anomie-regs#4212, anomie-timing#s-cpu/registers
.proc nmi
    lda a:$4210
    sta a:$0005                 ; RDNMI
    lda a:$4212
    sta a:$0006                 ; HVBJOY
    lda a:$4210
    sta a:$0007                 ; RDNMI, flag cleared
    rti
.endproc

; COP, BRK and ABORT are not used: stop if one is taken.
; @source: wdc-w65c816s#7.14
.proc halt
    stp
.endproc
