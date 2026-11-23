; BG mode 7 (#93, ADR 0032): a matrix of A = D = 2.0 and a scroll of -8 show the tilemap's
; first four columns at half size, with the column before them, which wraps from the far
; edge of the 1024-pixel playfield, on its left; D = 2.0 too, so line 4 leaves the first
; tile row. M7SEL is left at 0, so the playfield wraps. It checks nothing itself: a Rust
; test in sfes-snes-core compares the picture with values from the documents (ADR 0013).
; @source: anomie-regs#2105, anomie-regs#210d, anomie-regs#211a, anomie-regs#211b
; @source: anomie-regs#211c, anomie-regs#211d, anomie-regs#211e, anomie-regs#211f
; @source: anomie-regs#2120, anomie-regs#212c, anomie-regs#backgrounds/mode-7
; @source: fullsnes#snesppurotationscaling, fullsnes#snesppubgcontrol
; @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
; @source: adr-0013, adr-0032

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES BG MODE 7", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). In mode 7 each VRAM
; word holds a tilemap entry in its low byte and a pixel of a character in its high byte;
; a character is 64 words, and the tilemap is 128 entries to a row (anomie-regs#backgrounds/
; mode-7). Words 0 to 7 are the tilemap's first entries, which are also character 0's first
; row: entries 1, 2 and 3 for columns 0 to 2, and pixels 0. Words 64 to 127 are character 1,
; words 128 to 191 character 2, and so on, and are also tilemap entries 64 to 127 and the
; second row's first 64, of which only entry 127 is not 0: character 4. Character c is the
; solid colour c.
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#210d, anomie-regs#211a
; @source: anomie-regs#211b, anomie-regs#211c, anomie-regs#211d, anomie-regs#211e
; @source: anomie-regs#211f, anomie-regs#2120, anomie-regs#212c
; @source: anomie-regs#backgrounds/mode-7
.proc reset
    clear_vram

    ppu_write $2105, $07        ; BGMODE: mode 7

    vram_at $0000
    vram_word $0001             ; entry 0: character 1
    vram_word $0002             ; entry 1: character 2
    vram_word $0003             ; entry 2: character 3
    .repeat 5
        vram_word $0000
    .endrepeat
    .repeat 56                  ; the rest of character 0: transparent
        vram_word $0000
    .endrepeat
    .repeat 63                  ; character 1, with entries 64 to 126 clear
        vram_word $0100
    .endrepeat
    vram_word $0104             ; its last pixel, and entry 127: character 4
    .repeat 64                  ; character 2
        vram_word $0200
    .endrepeat
    .repeat 64                  ; character 3
        vram_word $0300
    .endrepeat
    .repeat 64                  ; character 4
        vram_word $0400
    .endrepeat

    ; M7A = M7D = $0200 (2.0), M7B = M7C = 0, written low byte first (anomie-regs#211b).
    ppu_write $211B, $00
    ppu_write $211B, $02
    ppu_write $211C, $00
    ppu_write $211C, $00
    ppu_write $211D, $00
    ppu_write $211D, $00
    ppu_write $211E, $00
    ppu_write $211E, $02
    ; M7X = M7Y = 0, M7HOFS = -8 ($1FF8, 13 bits) and M7VOFS = 0.
    ppu_write $211F, $00
    ppu_write $211F, $00
    ppu_write $2120, $00
    ppu_write $2120, $00
    ppu_write $210D, $F8
    ppu_write $210D, $1F
    ppu_write $210E, $00
    ppu_write $210E, $00

    color 0, $7FFF              ; backdrop: white
    color 1, $001F              ; character 1: red
    color 2, $7C00              ; character 2: blue
    color 3, $03E0              ; character 3: green
    color 4, $7C1F              ; character 4: magenta

    ppu_write $212C, $01        ; TM: BG1
    display_on
.endproc

; Every other vector stops the CPU too, so an unexpected interrupt is visible as a stop.
; @source: wdc-w65c816s#7.14
.proc halt
    stp
.endproc

.segment "RODATA"

; One zero byte for the fixed-address VRAM clear (ADR 0013).
; @source: adr-0013
zero:
    .byte $00
