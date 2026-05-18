NASM   ?= nasm
NEETAN ?= cargo run --release --manifest-path ../../Cargo.toml --quiet --

all: demo_01.d88

demo_01.bin: demo_01.asm
	$(NASM) -f bin -o $@ $<
	@test "$$(wc -c < $@ | tr -d ' ')" = "1024"

%.d88: %.bin
	rm -f $@
	$(NEETAN) create-fdd $@ --type 2hd
	dd if=$< of=$@ bs=16 count=64 seek=44 conv=notrunc 2>/dev/null

clean:
	rm -f demo_01.bin demo_01.d88

.PHONY: all clean
