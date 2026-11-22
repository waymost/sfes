# 0029. PPU memory ports, multiplier and status registers

- **Status:** accepted
- **Date:** 2026-11-22
- **Confidence:**
  - high: the VRAM, CGRAM and OAM port mechanics, the multiplier, and the bits of `$213E` and `$213F` (points 3 to 5, 8 to 10);
  - medium: when the OAM address is reloaded (point 6), and the reset value of `$2100` (point 2);
  - low: what an access outside its display period does (point 7), the PPU2 version number (point 10), and which reads clear the latch flag (point 10).

## Context

Issue #90 implements the PPU registers that ADR 0028, point 3 left as CPU open bus:

- the VRAM, CGRAM and OAM ports, `$2102–$2104`, `$2115–$2119`, `$2121`, `$2122`, `$2138–$213B`;
- the multiply result `$2134–$2136`;
- the status registers `$213E STAT77` and `$213F STAT78`.

It also covers the OAM address reset and the restrictions on when the memories can be accessed. Several of these rest on facts the PPU cannot know by itself in this design: the position of the beam, which the machine's scheduler keeps (ADR 0020); the interlace field (ADR 0020, point 4); the console's video standard; and `$4201` bit 7, which lives in the CPU (ADR 0021, point 7).

The documents agree on the port mechanics. They are silent or disagree on:

- what an access outside its allowed period does;
- when the OAM address is reloaded;
- which `$213F` reads clear the latch flag;
- the PPU2 version number.

## Sources

- **anomie-regs#registers** (the preamble): the flag columns `fvha` say whether a register "can be read/written" in force blank, V-Blank, H-Blank and at any time, '+' if it can and '-' otherwise. "It may be possible to write registers anytime even if marked '-', but until we have proof '-' is a better guess." A '?' is not explained.
- **anomie-regs#2102, #2103**: `$2103` bit 7 is the priority bit; `b aaaaaaaa` is a 9-bit word address. Writing either register "resets the entire internal OAM Address to the values last written". The internal address is invalidated while lines are rendered, by a rule nobody knows, and "the last value written to these registers is reloaded into the internal OAM address at the beginning of V-Blank if that occurs outside of a force-blank period". Flags `wl++?-` and `wh++?-`.
- **anomie-regs#2104**: "the low table of OAM is not affected until the high byte of a word is written (however, the high table is affected immediately)"; the worked example "01 02 01 03". Writing during H-blank is possible, but the byte written "will probably not be what you expect". Flags `wb++--`.
- **anomie-regs#sprites/oam**: OAM is a 512-byte low table and a 32-byte high table; "only the low 4 bits of $2102 are significant for indexing this table". The internal address has 10 bits, "with bit 9 selecting the table". The reload also occurs "on a 1->0 transition of $2100.7". A second worked example: "Write 1, read, read, Write 2, read, write 3 => OAM is 00 00 01 02 01 03".
- **anomie-regs#2100**: writing `$2100` on the first line of V-Blank while force blank is active "causes the OAM Address Reset to occur".
- **anomie-timing#oam-reset**: at the beginning of V-Blank, if force blank is off, the internal OAM address is reset to the value last written to `$2102–$2103`; the doc reports a claim that this occurs at H=10 of line 225 or 240, and on any 1→0 transition of `$2100` bit 7.
- **fullsnes#snesmemoryoamaccessspriteattributes**: a 9-bit reload value and a 10-bit address register; writing `$2102` or `$2103` copies the reload value to the address "with address Bit0=0". The reload at the beginning of V-Blank happens "only if not in Forced Blank mode", and "also when deactivating forced blank anytime during the first scanline of vblank". A table: a write to an even address sets `OAM_Lsb`; a write to an odd address below `$200` sets the word to `Data*256 + OAM_Lsb`; a write to any address above `$1FF` sets the byte; a read returns the byte. Addresses `$220–$3FF` mirror `$200–$21F`.
- **anomie-regs#2115**: bit 7 chooses whether the address increments after `$2118`/`$2139` or after `$2119`/`$213A`; bits 1–0 the step, 1, 32, 128, 128; bits 3–2 remap `aaaaaaaaBBBccccc` to `aaaaaaaacccccBBB` and the 9- and 10-bit forms. "If $2116/7 are set to #$0003, then word address #$0018 will be written instead, and $2116/7 will be incremented to $0004." Flags `wb++?-`.
- **anomie-regs#2116, #2117**: a word address for `$2118/9` and `$2139/a`. Flags `wl++?-`, `wh++?-`.
- **anomie-regs#2118, #2119**: a write takes effect at once, and the address increments on the byte `$2115` bit 7 names. Flags `wl++--`, `wh++--`.
- **anomie-regs#2139, #213a**: a read returns a byte of a word buffer; "a word from VRAM is loaded into this buffer just *before* the VRAM address is incremented". Flags `r l++?-`, `r h++?-`.
- **fullsnes#snesmemoryvramaccesstileandbgmap**: translation rotates the low 8, 9 or 10 bits of the word address left by three, and applies "only temporarily upon memory accesses". Address bit 15 is not connected, so `$8000–$FFFF` mirror `$0000–$7FFF`. The prefetch register is loaded from the translated address after a write to `$2116/$2117`, and before an increment caused by a read; a read first returns a byte of the old prefetch value. Writes to `$2118/$2119` leave the prefetch register "totally unchanged".
- **anomie-regs#2121, #2122, #213b**: `$2121` is a word address. CGRAM accesses "are handled just like accesses to the low table of OAM". In `$213B` the high byte's bit 7 is PPU2 open bus. Flags `wb+++-`, `ww+++-`, `r w++?-`.
- **fullsnes#snesmemorycgramaccesspalettememory**: writing `$2121` resets the first/second access flip-flop. The second access carries the upper 7 bits, "upper 1bit = PPU2 open bus". An even write sets `Cgram_Lsb`, an odd write sets the word to `Data*256 + Cgram_Lsb`, and a read returns the byte. The address increments after every access.
- **fullsnes#snesppucontrol**: in forced blank, VRAM, OAM and CGRAM "can be freely accessed (otherwise it's accessible only during Vblank)".
- **fullsnes#snestimingppumemoryaccesses**: CGRAM can be changed "during certain timespots in the Hblank-period", sometimes; OAM changes during H-blank are possible at best; the OAM address "is scattered during drawing period".
- **anomie-timing#s-ppu/blanking-periods**: V-Blank runs from H=0 of line 225 or 240 to H=0 of line 0, and H-Blank from H=274 to H=1.
- **anomie-regs#211b, #211c**: `$211B–$2120` are write-twice registers, `Reg = (Current<<8) | Prev; Prev = Current`, with one `Prev` shared by all of them and by the M7HOFS/M7VOFS halves of `$210D/$210E`. "The product A*(B>>8) may be read from registers $2134/6."
- **anomie-regs#2134, #2135, #2136**: the two's-complement product of the 16-bit `$211B` and "the 8-bit value most recently written to $211c"; "It may not be operative during Mode 7 rendering." Flags `r l+++?` and so on.
- **fullsnes#snesmathsmultiplydivide** ("PPU Ports"): a signed 16-bit multiplicand times a signed 8-bit multiplier; the result is readable immediately after either write. In BG mode 7 the registers are usable only in V-Blank and forced blank and otherwise "return garbage".
- **anomie-timing#s-ppu/registers**: "Some unknown number of master cycles after $211c is set, the product may be read".
- **anomie-regs#213e, fullsnes#snespputimersandstatus**: `$213E` is time over (bit 7), range over (bit 6), master/slave (bit 5, always 0 on a console), PPU1 open bus (bit 4) and the 5C77 version, "only version 1". `$213F` is the field (bit 7), the external latch flag (bit 6), PPU2 open bus (bit 5), NTSC/PAL (bit 4) and the 5C78 version: anomie has seen "at least 2 and 3. Possibly 1 as well", fullsnes "version 1..3". A `$213F` read resets both counter flip-flops. anomie: the latch flag "is reset on read, but only when $4201 bit 7 is set"; fullsnes: "Reading from this register also resets the latch flag", with no condition.
- **fullsnes#snesiomap**: `$2100` is "8xh" at reset, not in brackets; `$2115` is "(?Fh)"; `$211B` and `$211C` are "(FFh)(w2)"; `$2134–$2136` are "(01h)", "(00h)", "(00h)"; the other port registers are "(?)"; `$213F` bit 7 is 0.

No test ROM or oracle output settles the open questions yet.

## Decision

1. **What the PPU sees from the machine.** Before each access to a PPU register the bus syncs the PPU (ADR 0028, point 6) and passes it `Signals`:
   - the display period: vertical blank (ADR 0020), else horizontal blank from H=274 to H=1, else active display;
   - the field, `$213F` bit 7 (ADR 0020, point 4);
   - whether the console is PAL;
   - whether `$4201` bit 7 is set.

   A peek, which cannot sync, uses signals worked out at that moment.
2. **`$2100 INIDISP`.** The PPU keeps it. It is `$80` at power-on and on reset: fullsnes's "8xh", with the unknown brightness taken as 0, as ADR 0028, point 5 does for `$2105`. Bit 7 is forced blank for points 6 and 7. Brightness and the forced-blank picture wait for #96.
3. **The VRAM port.**
   - `$2115` is decoded on write: the increment byte (bit 7), the translation (bits 3–2) and the step (bits 1–0, 1, 32, 128, 128). It is `$0F` at power-on, fullsnes's "?Fh" with the unknown high nibble taken as 0, and is kept on reset.
   - `$2116/$2117` set the low and high byte of a 16-bit word address, then load the prefetch register from it.
   - A memory access uses the address translated by `$2115` bits 3–2, with bit 15 dropped. The translation never changes the address register.
   - `$2118` writes the low byte of the word, `$2119` the high byte. The address then increments by the step if `$2115` bit 7 names that byte. Writes never touch the prefetch register.
   - `$2139` returns the prefetch register's low byte, `$213A` its high byte. If `$2115` bit 7 names that byte, the prefetch register is then loaded from the current address and the address increments.
   - The address increments with 16-bit wrap-around.
4. **The CGRAM port.**
   - `$2121` sets the word address, so the byte address is twice it, and resets the flip-flop to the first access.
   - `$2122` at an even byte address sets the latch. At an odd one it writes the latch to the even byte and the value, bit 7 dropped, to the odd byte.
   - `$213B` returns the byte at the address. At an odd address bit 7 is PPU2 open bus.
   - Each access increments the 9-bit byte address, wrapping at 512.
5. **The OAM port.**
   - `$2102` sets bits 7–0 of the 9-bit reload value and `$2103` bit 8; `$2103` bit 7 is kept as the priority bit for #94. Either write copies the reload value, doubled, into the 10-bit byte address.
   - `$2104` at an even address sets the latch. At an odd address below `$200` it writes the latch to the even byte and the value to the odd byte. At any address from `$200` up it writes the value to the high table byte `$200 + (address & $1F)`.
   - `$2138` returns the byte at the address, the high table mirrored the same way.
   - Each access increments the address, wrapping at `$400`.
   - Both worked examples in anomie-regs follow from these rules, and are unit tests.
6. **The OAM address reload.** The address is reloaded from the reload value:
   - at the start of vertical blank, if forced blank is off;
   - on a write to `$2100` that clears bit 7 while it was set, at any time.

   The second rule is anomie's (`#sprites/oam`, `anomie-timing#oam-reset`); it includes fullsnes's narrower case, deactivating forced blank on the first line of vertical blank. Not modelled:
   - the reported H=10 for the first rule; the reload happens at H=0, with the frame's other vertical-blank events;
   - anomie-regs#2100's reload on any write to `$2100` during that line while forced blank is on;
   - the address's invalidation during rendering, which no source describes.
7. **Access by display period.** Taking anomie's '-' as the rule, and fullsnes's "otherwise it's accessible only during Vblank":
   - A write to VRAM (`$2118/$2119`) or OAM (`$2104`) changes the memory only in vertical blank or forced blank. A write to CGRAM (`$2122`) changes it in those periods or in horizontal blank, where anomie's flags allow it.
   - A refused write changes no memory, but the port runs as usual: the address increments and the latches are set as point 3, 4 or 5 says.
   - Reads are not restricted. No source says what a read outside its period returns, so it returns the memory as it stands.
   - The address and control registers, `$2102`, `$2103`, `$2115–$2117` and `$2121`, are written at any time.
8. **The multiplier.**
   - The PPU keeps one write-twice latch, shared by `$210D`, `$210E` and `$211B–$2120`. A write to any of them makes that register `(value << 8) | latch` and then sets the latch to the value. Until #91 and #93 only `$211B` M7A and `$211C` M7B keep their values; the others only move the latch.
   - `$2134–$2136` read the low, middle and high byte of the 24-bit two's-complement product of M7A, as a signed 16-bit value, and M7B's high byte, the last value written to `$211C`, as a signed 8-bit value. The product is ready at once.
   - M7A and M7B are `$FFFF` at power-on, with the latch `$FF`, so the product is `$000001` as fullsnes lists. They are kept on reset.
   - The garbage that fullsnes reports in Mode 7 outside blanking is not modelled: the product is always correct.
9. **`$213E STAT77`.** Bits 7 and 6 are 0 until the sprite evaluation of #94 sets them. Bit 5 is 0, bit 4 is PPU1 open bus, and bits 3–0 are version 1.
10. **`$213F STAT78`.**
    - Bit 7 is the field. Bit 6 is the latch flag of ADR 0021, point 7. Bit 5 is PPU2 open bus. Bit 4 is set on a PAL console. Bits 3–0 are version 3, the later of the two versions anomie has seen; no source says which consoles carry which.
    - A read resets both counter flip-flops to the low byte.
    - It clears the latch flag only while `$4201` bit 7 is set, as anomie says. fullsnes gives no condition. Anomie's rule is the more specific claim, and it matches the latch itself, which also works only while that bit is set.
11. **Open bus and side effects.** Each read of `$2134–$2136`, `$2138–$213A` and `$213E` sets the PPU1 open-bus latch to the byte returned. Each read of `$213B–$213D` and `$213F` sets the PPU2 one (ADR 0028, point 4). Every one of these reads therefore has side effects, and a peek reports it.
12. **Power-on and reset.** fullsnes lists every other port value as "(?)": unchanged on reset, power-on value unknown. They are 0 at power-on: the VRAM address and prefetch, the CGRAM address, flip-flop and latch, the OAM reload value, priority bit, address and latch. Reset keeps them. These are project choices, like ADR 0028's open-bus latches.

## Consequences

- ADR 0028, point 3's rule for `$2134–$2136`, `$2138–$213B`, `$213E` and `$213F` is replaced by points 3 to 11. With it goes the last case of ADR 0019, point 1's open-bus rule for the PPU. ADR 0021, point 7's missing `$213F` read is point 10.
- A ROM can now load VRAM, CGRAM and OAM. The backdrop is no longer seeded garbage once a ROM writes CGRAM entry 0, so own test carts that check the backdrop become possible.
- `Signals` makes the PPU's view of time explicit. A later issue that needs the dot itself, for sprite evaluation or the Mode 7 garbage, extends `Signals`.
- To revisit:
  - point 7, when a test ROM or oracle shows what VRAM or OAM accesses during active display do;
  - point 6's H=0, when a ROM that times OAM writes against the start of vertical blank disagrees;
  - point 10's version and latch rule, when MesenCE or a hardware test is compared.
