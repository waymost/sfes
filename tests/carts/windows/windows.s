; Windows (#95, ADR 0034): BG1 and BG2 are solid in the first 40 pixels and BG3 in the last
; 24, each one colour. Window 1 is pixels 8 to 23 and window 2 is 16 to 31. BG1 is masked
; where exactly one window is (both enabled, XOR); BG2 is masked outside window 1
; (inverted); BG3 has an inverted window but is not designated in $212E, so it is not
; masked. It checks nothing itself: a Rust test in sfes-snes-core compares the picture with
; values from the documents (ADR 0013).
; @source: anomie-regs#2105, anomie-regs#2107, anomie-regs#2108, anomie-regs#2109
; @source: anomie-regs#210b, anomie-regs#210c, anomie-regs#212c
; @source: anomie-regs#2123, anomie-regs#2124, anomie-regs#2126, anomie-regs#2127
; @source: anomie-regs#2128, anomie-regs#2129, anomie-regs#212a, anomie-regs#212e
; @source: anomie-regs#backgrounds/mode-0, anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: anomie-regs#windows, fullsnes#snesppuwindow
; @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
; @source: adr-0013, adr-0034

.p816
.include "header.inc"
.include "ppu.inc"

cart_header "SFES WINDOWS", $20, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector (wdc-w65c816s#2.25). VRAM, in words:
;   $0400 BG1 map, $0800 BG2 map, $0C00 BG3 map
;   $4000 BG1 characters, $5000 BG2, $6000 BG3 (tile 1 is 8 words in)
; Every tile is colour 1 in every pixel (plane 0 set, plane 1 clear), palette 0: BG1's is
; CGRAM entry 1, BG2's is 32 + 1 and BG3's is 64 + 1 (anomie-regs#backgrounds/mode-0). BG1
; and BG2 fill map columns 0 to 4 (pixels 0 to 39), BG3 columns 5 to 7 (pixels 40 to 63).
; The window setting nibbles (anomie-regs#2123): BG1 `$A` is both windows enabled, not
; inverted, so $2123's low nibble; BG2 `$3` is window 1 enabled and inverted, its high
; nibble; BG3 `$3` is the same in $2124's low nibble. The logic of BG1 is 2, XOR
; ($212A bits 1 and 0). TMW $03 designates BG1 and BG2 only.
; @source: wdc-w65c816s#2.25, anomie-regs#2105, anomie-regs#2107, anomie-regs#2108
; @source: anomie-regs#2109, anomie-regs#210b, anomie-regs#210c, anomie-regs#212c
; @source: anomie-regs#2123, anomie-regs#2124, anomie-regs#2126, anomie-regs#2127
; @source: anomie-regs#2128, anomie-regs#2129, anomie-regs#212a, anomie-regs#212e
; @source: anomie-regs#backgrounds/mode-0
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps, anomie-regs#windows
.proc reset
    clear_vram

    ppu_write $2105, $00        ; BGMODE: mode 0, 8x8 tiles
    ppu_write $2107, $04        ; BG1SC: word $0400, 32x32
    ppu_write $2108, $08        ; BG2SC
    ppu_write $2109, $0C        ; BG3SC
    ppu_write $210B, $54        ; BG12NBA: BG1 at word $4000, BG2 at $5000
    ppu_write $210C, $06        ; BG34NBA: BG3 at $6000

    vram_at $4008
    vram_rows $00FF             ; BG1 tile 1: colour 1
    vram_at $5008
    vram_rows $00FF             ; BG2
    vram_at $6008
    vram_rows $00FF             ; BG3

    vram_at $0400
    .repeat 5
        vram_word $0001         ; BG1 columns 0 to 4: tile 1, palette 0
    .endrepeat
    vram_at $0800
    .repeat 5
        vram_word $0001         ; BG2 columns 0 to 4
    .endrepeat
    vram_at $0C05
    .repeat 3
        vram_word $0001         ; BG3 columns 5 to 7
    .endrepeat

    color 0, $7C00              ; backdrop: blue
    color 1, $001F              ; BG1: red
    color 33, $03E0             ; BG2: green
    color 65, $7FFF             ; BG3: white

    ppu_write $2126, 8          ; WH0: window 1 left
    ppu_write $2127, 23         ; WH1: window 1 right
    ppu_write $2128, 16         ; WH2: window 2 left
    ppu_write $2129, 31         ; WH3: window 2 right
    ppu_write $2123, $3A        ; W12SEL: BG2 `$3` in the high nibble, BG1 `$A` in the low
    ppu_write $2124, $03        ; W34SEL: BG3 `$3`
    ppu_write $212A, $02        ; WBGLOG: BG1 uses XOR
    ppu_write $212E, $03        ; TMW: BG1 and BG2 are masked in their windows

    ppu_write $212C, $07        ; TM: BG1 to BG3 on the main screen
    display_on
.endproc

.include "halt.inc"
