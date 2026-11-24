; The controller ports and the auto-joypad read (#84, ADR 0024). The cartridge latches the
; pads through $4016 and reads port 1 sixteen times and once more, port 2 twice, and port 1
; three times with the latch held high. It then enables the auto-read, waits for $4212
; bit 0 to set and clear, and copies $4218-$421F. It checks nothing itself: a Rust test in
; sfes-snes-core sets the buttons and compares the WRAM it leaves with values from the
; documents (ADR 0013).
; @source: anomie-regs#controllers/generic, anomie-regs#controllers/joypads
; @source: anomie-regs#4016, anomie-regs#4017, anomie-regs#4200, anomie-regs#4212
; @source: anomie-regs#4218, anomie-timing#s-cpu/auto-joypad-read
; @source: fullsnes#snescontrollersioportsmanualreading
; @source: fullsnes#snescontrollersioportsautomaticreading, adr-0013, adr-0024

.p816
.include "header.inc"

cart_header "SFES JOYPAD TEST", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector with D = $0000, DBR = $00 and M = X = 1
; (wdc-w65c816s#2.25), so every access is 8-bit and absolute addresses are in bank $00,
; whose $0000-$1FFF is WRAM (anomie-memmap#memory-map). LDA abs fetches the address's
; high byte, $40, in the cycle before its read (wdc-w65c816s#table-5-7-1a), so $40 is on
; the data bus when $4016 or $4017 is read and fills their open-bus bits
; (anomie-memmap#hardware/data-bus; ADR 0024, point 4). WRAM it leaves:
;   $0000-$0010  $4016 after a latch: port 1's 16 bits, then a 17th, 1 (anomie-regs#4016)
;   $0011-$0012  $4017: port 2's first two bits, with bits 4-2 set (anomie-regs#4017)
;   $0013-$0015  $4016 with the latch held high: B each time
;                (fullsnes#snescontrollersioportsautomaticreading)
;   $0016-$001D  $4218-$421F after one auto-read (anomie-regs#4218)
; @source: wdc-w65c816s#2.25, anomie-memmap#memory-map, wdc-w65c816s#table-5-7-1a
; @source: anomie-memmap#hardware/data-bus, anomie-regs#4016, anomie-regs#4017
; @source: anomie-regs#4200, anomie-regs#4212, anomie-regs#4218
; @source: anomie-timing#s-cpu/auto-joypad-read
; @source: fullsnes#snescontrollersioportsautomaticreading, adr-0024
.proc reset
    ; Latch both pads, then read port 1 seventeen times.
    lda #$01
    sta a:$4016                 ; JOYSER0: latch high
    stz a:$4016                 ; latch low
    ldx #0
@manual:
    lda a:$4016                 ; JOYSER0: port 1 Data1 in bit 0
    sta a:$0000,x
    inx
    cpx #17
    bne @manual

    ; Port 2 has not been clocked: its first two bits.
    lda a:$4017                 ; JOYSER1
    sta a:$0011
    lda a:$4017
    sta a:$0012

    ; With the latch held high every read returns B.
    lda #$01
    sta a:$4016
    lda a:$4016
    sta a:$0013
    lda a:$4016
    sta a:$0014
    lda a:$4016
    sta a:$0015
    stz a:$4016

    ; Enable the auto-read and wait for one to run (anomie-regs#4212).
    lda #$01
    sta a:$4200                 ; NMITIMEN: auto-joypad read on
@busy:
    lda a:$4212                 ; HVBJOY bit 0: auto-read in progress
    and #$01
    beq @busy
@done:
    lda a:$4212
    and #$01
    bne @done

    ; Copy $4218-$421F.
    ldx #0
@copy:
    lda a:$4218,x               ; JOY1L-JOY4H
    sta a:$0016,x
    inx
    cpx #8
    bne @copy
    stp
.endproc

.include "halt.inc"
