NEETAN ?= cargo run --release --manifest-path ../../Cargo.toml --quiet --
NASM   ?= nasm

all: dhrystone_2dd.d88 dhrystone_2hd.d88

dhrystone_2dd.bin: dhrystone.asm
	$(NASM) -f bin -DFDD_TYPE=2DD -o $@ $<

dhrystone_2hd.bin: dhrystone.asm
	$(NASM) -f bin -DFDD_TYPE=2HD -o $@ $<

# 2DD layout used by the d88 image:
#   - File header (HEADER_SIZE) = 0x2B0 = 688 bytes.
#   - Each sector record = 16-byte header + 256-byte data = 272 bytes.
#   - Each track = 16 sectors = 4352 bytes.
# Sector i (0-indexed) in track t (0-indexed) has its DATA at file offset
# 688 + t*4352 + i*272 + 16. All offsets are multiples of 16, so we can
# write each 256-byte chunk in place with `dd bs=16 count=16` without
# clobbering the surrounding sector headers (which carry C/H/R/N metadata
# the FDC needs).
dhrystone_2dd.d88: dhrystone_2dd.bin
	rm -f $@
	$(NEETAN) create-fdd $@ --type 2dd
	@bin_size=`wc -c < $<`; \
	bin_size=`echo $$bin_size`; \
	sectors=`expr \( $$bin_size + 255 \) / 256`; \
	i=0; \
	while [ $$i -lt $$sectors ]; do \
	  track=`expr $$i / 16`; \
	  sect=`expr $$i % 16`; \
	  off=`expr 688 + $$track \* 4352 + $$sect \* 272 + 16`; \
	  seek_b=`expr $$off / 16`; \
	  skip_b=`expr $$i \* 16`; \
	  dd if=$< of=$@ bs=16 count=16 skip=$$skip_b seek=$$seek_b conv=notrunc 2>/dev/null; \
	  i=`expr $$i + 1`; \
	done
	rm -f $<

# 2HD layout used by the d88 image:
#   - File header (HEADER_SIZE) = 0x2B0 = 688 bytes.
#   - Each sector record = 16-byte header + 1024-byte data = 1040 bytes.
#   - Each track = 8 sectors = 8320 bytes.
# Sector i (0-indexed) in track t (0-indexed) has its DATA at file offset
# 688 + t*8320 + i*1040 + 16. We copy 1024 bytes as 64 blocks of 16 bytes
# each, which keeps `dd` aligned to the 16-byte boundary the surrounding
# sector headers depend on.
dhrystone_2hd.d88: dhrystone_2hd.bin
	rm -f $@
	$(NEETAN) create-fdd $@ --type 2hd
	@bin_size=`wc -c < $<`; \
	bin_size=`echo $$bin_size`; \
	sectors=`expr \( $$bin_size + 1023 \) / 1024`; \
	i=0; \
	while [ $$i -lt $$sectors ]; do \
	  track=`expr $$i / 8`; \
	  sect=`expr $$i % 8`; \
	  off=`expr 688 + $$track \* 8320 + $$sect \* 1040 + 16`; \
	  seek_b=`expr $$off / 16`; \
	  skip_b=`expr $$i \* 64`; \
	  dd if=$< of=$@ bs=16 count=64 skip=$$skip_b seek=$$seek_b conv=notrunc 2>/dev/null; \
	  i=`expr $$i + 1`; \
	done
	rm -f $<

clean:
	rm -f dhrystone_2dd.bin dhrystone_2dd.d88 dhrystone_2hd.bin dhrystone_2hd.d88

.PHONY: all clean
