; PPU setup macros shared by the own test cartridges that draw (ADR 0013, ADR 0030). Every
; access is 8-bit and absolute in bank $00, as in the cartridges' reset state
; (wdc-w65c816s#2.25). The PPU starts in forced blank ($2100 = $80, ADR 0029 point 2), where
; VRAM and CGRAM accept writes (anomie-regs#2100).
; @source: wdc-w65c816s#2.25, anomie-regs#2100, anomie-regs#2115, anomie-regs#2118
; @source: anomie-regs#2121, anomie-regs#2122, adr-0013, adr-0029

; ppu_write register, value
; @source: anomie-regs#2100
.macro ppu_write register, value
    lda #value
    sta a:register
.endmacro

; Clears all of VRAM with one fixed-address DMA of 64 KiB from a zero byte in ROM: mode 1
; alternates $2118 and $2119, and VMAIN increments after $2119 (anomie-regs#2115, #43x0).
; The power-on contents are random (ADR 0028, point 1), so a tile's rows must be written.
; @source: anomie-regs#2115, anomie-regs#2116, anomie-regs#43x0, anomie-regs#420b
; @source: adr-0028
.macro clear_vram
    ppu_write $2115, $80        ; VMAIN: increment after $2119
    stz a:$2116                 ; VMADDL
    stz a:$2117                 ; VMADDH
    ppu_write $4300, $09        ; DMAP0: fixed A address, mode 1
    ppu_write $4301, $18        ; BBAD0: $2118
    ppu_write $4302, <zero      ; A1T0L
    ppu_write $4303, >zero      ; A1T0H
    stz a:$4304                 ; A1B0: bank $00
    stz a:$4305                 ; DAS0L: a count of 0 is 65536
    stz a:$4306                 ; DAS0H
    ppu_write $420B, $01        ; MDMAEN: channel 0
.endmacro

; vram_at word_address: VMADD, with VMAIN = $80 set by clear_vram.
; @source: anomie-regs#2116
.macro vram_at word_address
    ppu_write $2116, <(word_address)
    ppu_write $2117, >(word_address)
.endmacro

; vram_word value: one word at the VRAM address, which then increments.
; @source: anomie-regs#2118
.macro vram_word value
    ppu_write $2118, <(value)
    ppu_write $2119, >(value)
.endmacro

; vram_rows value: eight identical words, the eight rows of one pair of a tile's planes
; (the low byte is the lower plane; anomie-regs#backgrounds/tile-maps-and-character-maps).
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
.macro vram_rows value
    .repeat 8
        vram_word value
    .endrepeat
.endmacro

; color index, value: a CGRAM entry (anomie-regs#2121, #2122; two writes of CGDATA).
; @source: anomie-regs#2121, anomie-regs#2122
.macro color index, value
    ppu_write $2121, index
    ppu_write $2122, <(value)
    ppu_write $2122, >(value)
.endmacro

; Ends the setup: the picture on, full brightness, and the CPU stopped so the Rust test
; can run frames (anomie-regs#2100).
; @source: anomie-regs#2100
.macro display_on
    ppu_write $2100, $0F
    stp
.endmacro

; Reads $4212 until `branch` falls through on bit 7, vertical blank, which the load puts
; in N: bpl waits for vertical blank to begin, bmi for it to end (anomie-regs#4212).
; @source: anomie-regs#4212, wdc-w65c816s#table-5-7-1a
.macro wait_vblank branch
.local loop
loop:
    lda a:$4212                 ; HVBJOY
    branch loop
.endmacro

; hdma_channel channel, control, port, table: a direct channel reading `table` in bank $00
; (anomie-regs#43x0, #43x1, #43x2, #43x4).
; @source: anomie-regs#43x0, anomie-regs#43x1, anomie-regs#43x2, anomie-regs#43x4
.macro hdma_channel channel, control, port, table
    ppu_write $4300 + channel * $10, control
    ppu_write $4301 + channel * $10, port
    ppu_write $4302 + channel * $10, <table
    ppu_write $4303 + channel * $10, >table
    stz a:$4304 + channel * $10
.endmacro

; sprite xpos, ypos, tile, attributes: one OAM record, written through $2104 (OAMDATA), which
; increments the address after each byte; the record is X, Y, tile, `vhoopppN`
; (anomie-regs#sprites/oam).
; @source: anomie-regs#2104, anomie-regs#sprites/oam
.macro sprite xpos, ypos, tile, attributes
    ppu_write $2104, xpos
    ppu_write $2104, ypos
    ppu_write $2104, tile
    ppu_write $2104, attributes
.endmacro

; solid_tile colour: one 4-bit tile of a single colour. Plane 0 is the low byte of the first
; eight words and plane 1 the high byte; planes 2 and 3 are the same in the next eight
; (anomie-regs#backgrounds/tile-maps-and-character-maps, fullsnes#snesppuvideomemoryvram).
; @source: anomie-regs#backgrounds/tile-maps-and-character-maps
; @source: fullsnes#snesppuvideomemoryvram
.macro solid_tile colour
    vram_rows (((((colour) >> 1) & 1) * $FF00) | (((colour) & 1) * $FF))
    vram_rows (((((colour) >> 3) & 1) * $FF00) | ((((colour) >> 2) & 1) * $FF))
.endmacro

; One zero byte in RODATA for clear_vram's fixed-address DMA (ADR 0013).
; @source: adr-0013
.pushseg
.segment "RODATA"
zero:
    .byte $00
.popseg
