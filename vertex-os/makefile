# Vertex OS
#
# `make` builds the bootable ISO; `make test_node` exercises the hosted
# prototype of the node graph that the kernel implements.

CC      ?= gcc
CFLAGS  ?= -Wall -Wextra -g -std=c11 -I.

.PHONY: all iso run run-uefi kernel test_node clean help

all: iso

# --- kernel / ISO -----------------------------------------------------------

kernel:
	./tools/build.sh build

iso:
	./tools/build.sh iso

run:
	./tools/build.sh run

run-uefi:
	./tools/build.sh run-uefi

# --- hosted node-graph prototype -------------------------------------------

test_node: graph/node.o kernel/test_node.o
	$(CC) $(CFLAGS) -o test_node graph/node.o kernel/test_node.o

graph/node.o: graph/node.c graph/node.h
	$(CC) $(CFLAGS) -c graph/node.c -o graph/node.o

kernel/test_node.o: kernel/test_node.c graph/node.h
	$(CC) $(CFLAGS) -c kernel/test_node.c -o kernel/test_node.o

clean:
	rm -f graph/*.o kernel/*.o test_node
	rm -rf build
	cd kernel && cargo clean

help:
	@echo "make iso        build the bootable ISO (build/vertex-os.iso)"
	@echo "make run        build and boot in QEMU (BIOS)"
	@echo "make run-uefi   build and boot in QEMU (UEFI)"
	@echo "make test_node  build and run the C node-graph prototype"
	@echo "make clean      remove build artefacts"
