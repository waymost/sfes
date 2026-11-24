; The vector handler shared by the own test cartridges (ADR 0013). Include it at the end of a
; cartridge's CODE segment, after its own procedures, and name `halt` for every vector the
; cartridge does not handle: an unexpected interrupt is then visible as a stop.
; @source: wdc-w65c816s#7.14, adr-0013

; @source: wdc-w65c816s#7.14
.proc halt
    stp
.endproc
