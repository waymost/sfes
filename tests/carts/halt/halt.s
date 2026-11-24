; The smallest cartridge: it boots, writes a marker to WRAM and stops (ADR 0013). The machine's
; reset and boot tests (#79) use it to see that the reset vector was followed.
; @source: adr-0013

.p816
.include "header.inc"

cart_header "SFES HALT", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector with D = $0000, DBR = $00 and M = 1
; (wdc-w65c816s#2.25), so the store below is 8-bit and goes to bank $00. Bank $00
; $0000-$1FFF mirrors WRAM $7E:0000-$1FFF (anomie-memmap#memory-map). The marker $A5 is this
; cartridge's own choice (adr-0013).
; @source: wdc-w65c816s#2.25, anomie-memmap#memory-map, wdc-w65c816s#7.14, adr-0013
.proc reset
    lda #$A5
    sta a:$0000                 ; WRAM $7E:0000 = $A5
    stp
.endproc

; Every other vector stops the CPU too, so an unexpected interrupt is visible as a stop
; without the marker.
.include "halt.inc"
