; The multiply and divide unit, WRIO and RDIO (#83, ADR 0023). The cartridge starts
; multiplications and divisions and reads their results one CPU cycle before and exactly
; when they appear, divides by zero, and reads $4213 before and after a $4201 write. It
; checks nothing itself: a Rust test in sfes-snes-core compares the WRAM it leaves with
; values computed from the documents (ADR 0013).
; @source: anomie-regs#4202, anomie-regs#4203, anomie-regs#4204, anomie-regs#4205
; @source: anomie-regs#4206, anomie-regs#4214, anomie-regs#4216, anomie-regs#4201
; @source: anomie-regs#4213, anomie-timing#s-cpu/registers, fullsnes#snesmathsmultiplydivide
; @source: fullsnes#snescontrollersioportsmanualreading, adr-0023, adr-0013

.p816
.include "header.inc"

cart_header "SFES MATH", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector with D = $0000, DBR = $00 and M = X = 1
; (wdc-w65c816s#2.25), so every access is 8-bit and absolute addresses are in bank $00,
; whose $0000-$1FFF is WRAM (anomie-memmap#memory-map). The unit counts CPU cycles, so
; refreshes do not move any result (ADR 0023, point 2). STA abs writes in its fourth
; cycle and LDA abs reads in its fourth (wdc-w65c816s#table-5-7-1a); NOP is two cycles
; (#table-5-7-19a) and XBA three (#table-5-7-19b). Counting from the write, a read after
; NOP NOP is the 8th cycle, after NOP XBA the 9th, after six NOPs the 16th and after five
; NOPs and XBA the 17th.
; @source: wdc-w65c816s#2.25, anomie-memmap#memory-map, wdc-w65c816s#table-5-7-1a
; @source: wdc-w65c816s#table-5-7-19a, wdc-w65c816s#table-5-7-19b
; @source: anomie-regs#4202, anomie-regs#4203, anomie-regs#4204, anomie-regs#4205
; @source: anomie-regs#4206, anomie-regs#4214, anomie-regs#4216, anomie-regs#4213
; @source: fullsnes#snesmathsmultiplydivide, adr-0023
.proc reset
    ; 200 x 250 = 50000 = $C350, read in the 8th cycle: still the power-on 0.
    lda #200
    sta a:$4202                 ; WRMPYA
    lda #250
    sta a:$4203                 ; WRMPYB: start
    nop
    nop
    lda a:$4216
    sta a:$0000                 ; RDMPYL, early
    lda a:$4216
    sta a:$0001                 ; RDMPYL
    lda a:$4217
    sta a:$0002                 ; RDMPYH
    lda a:$4214
    sta a:$0003                 ; RDDIVL: the multiplier
    lda a:$4215
    sta a:$0004                 ; RDDIVH: 0

    ; 200 x 3 = 600 = $0258 with $4202 kept, read in the 9th cycle.
    lda #3
    sta a:$4203                 ; WRMPYB: start
    nop
    xba
    lda a:$4216
    sta a:$0005                 ; RDMPYL, on time

    ; 12345 = $3039 = 123 x 100 + 45, read in the 16th cycle: still the multiplier 3.
    lda #$39
    sta a:$4204                 ; WRDIVL
    lda #$30
    sta a:$4205                 ; WRDIVH
    lda #100
    sta a:$4206                 ; WRDIVB: start
    nop
    nop
    nop
    nop
    nop
    nop
    lda a:$4214
    sta a:$0006                 ; RDDIVL, early
    lda a:$4214
    sta a:$0007                 ; RDDIVL
    lda a:$4215
    sta a:$0008                 ; RDDIVH
    lda a:$4216
    sta a:$0009                 ; RDMPYL: the remainder
    lda a:$4217
    sta a:$000A                 ; RDMPYH

    ; 12345 = 1763 x 7 + 4 with the dividend kept, 1763 = $06E3, read in the 17th cycle.
    lda #7
    sta a:$4206                 ; WRDIVB: start
    nop
    nop
    nop
    nop
    nop
    xba
    lda a:$4214
    sta a:$000B                 ; RDDIVL, on time
    lda a:$4216
    sta a:$000C                 ; RDMPYL: the remainder

    ; Dividing by zero gives $FFFF and the dividend; eight NOPs wait out the 16 cycles.
    stz a:$4206                 ; WRDIVB: start
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    lda a:$4214
    sta a:$000D                 ; RDDIVL
    lda a:$4215
    sta a:$000E                 ; RDDIVH
    lda a:$4216
    sta a:$000F                 ; RDMPYL
    lda a:$4217
    sta a:$0010                 ; RDMPYH

    ; RDIO reads $4201 back: $FF from power-on, then the value written.
    lda a:$4213
    sta a:$0011                 ; RDIO
    lda #$40
    sta a:$4201                 ; WRIO
    lda a:$4213
    sta a:$0012                 ; RDIO
    stp
.endproc

; No interrupt is enabled: stop if one is taken.
; @source: wdc-w65c816s#7.14
.proc halt
    stp
.endproc
