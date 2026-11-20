; Bus decode, access speed and the WRAM port (#80, ADR 0019). The cartridge switches to
; FastROM and runs a few instructions from bank $80, drives the WRAM port at $2180-$2183,
; reads a write-only register and runs INC $2180 in emulation mode, then stops. It checks
; nothing itself: a Rust test in sfes-snes-core compares the WRAM it leaves and the start
; time of each instruction with values computed from the documents (ADR 0013).
; @source: anomie-memmap#memory-map
; @source: anomie-regs#2180, anomie-regs#2181, anomie-regs#420d
; @source: adr-0019
; @source: adr-0013

.p816
.include "header.inc"

cart_header "SFES BUS TIMING", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector with D = $0000, DBR = $00 and M = X = 1
; (wdc-w65c816s#2.25), so every access below is 8-bit and absolute addresses are in bank
; $00, whose $0000-$1FFF is WRAM $7E:0000-$1FFF (anomie-memmap#memory-map).
; @source: wdc-w65c816s#2.25, anomie-memmap#memory-map, anomie-regs#420d
; @source: fullsnes#snesmemorycontrol, wdc-w65c816s#table-5-7-4b
.proc reset
    lda #$01
    sta a:$420D                 ; MEMSEL: Memory-2 ($80-$BF:8000-FFFF) is now Fast
    jml $800000 | fast          ; the same ROM bytes, seen through bank $80
.endproc

; Runs from bank $80 while MEMSEL bit 0 is set, so each opcode and operand fetch takes 6
; master cycles instead of 8 (anomie-memmap#memory-map, note 2). The marker $A5 at
; $7E:0000 is this cartridge's own choice (adr-0013).
; @source: anomie-memmap#memory-map, fullsnes#snesmemorycontrol, adr-0013
.proc fast
    lda #$A5
    sta a:$0000                 ; WRAM $7E:0000 = $A5, a Slow access
    jml slow                    ; back to bank $00, where ROM is Slow
.endproc

; @source: anomie-regs#2180, anomie-regs#2181, fullsnes#snesmemoryworkramaccess
; @source: anomie-memmap#hardware/data-bus, adr-0006, adr-0019
.proc slow
    ; The WRAM port: address $1:0100 is WRAM $7F:0100.
    stz a:$2181                 ; WMADDL = $00
    lda #$01
    sta a:$2182                 ; WMADDM = $01
    sta a:$2183                 ; WMADDH = $01
    lda #$5A
    sta a:$2180                 ; $7F:0100 = $5A, address $1:0101
    lda #$C3
    sta a:$2180                 ; $7F:0101 = $C3, address $1:0102
    ; Read both back through the port.
    stz a:$2181                 ; address $1:0100
    lda a:$2180
    sta a:$0001                 ; $7E:0001 = $5A
    lda a:$2180
    sta a:$0002                 ; $7E:0002 = $C3
    ; WMADDL is write-only, so the read returns the last value on the data bus: the high
    ; byte of the instruction's own operand, $21 (anomie-regs#2181).
    lda a:$2181
    sta a:$0003                 ; $7E:0003 = $21
    ; INC $2180 in emulation mode writes the value read, then the result (ADR 0006,
    ; ADR 0019 point 7): $7F:0200 is read, $7F:0201 gets $40 and $7F:0202 gets $41.
    lda #$40
    sta f:$7F0200
    stz a:$2181                 ; WMADDL = $00: the read-back left it at $02
    lda #$02
    sta a:$2182                 ; address $1:0200
    inc a:$2180
    stp
.endproc

; Every other vector stops the CPU too, so an unexpected interrupt is visible as a stop
; without the marker.
; @source: wdc-w65c816s#7.14
.proc halt
    stp
.endproc
