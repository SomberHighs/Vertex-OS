CC = gcc
CFLAGS = -Wall -Wextra -g -std=c11 -I.

test_node: graph/node.o kernel/test_node.o
	$(CC) $(CFLAGS) -o test_node graph/node.o kernel/test_node.o

graph/node.o: graph/node.c graph/node.h
	$(CC) $(CFLAGS) -c graph/node.c -o graph/node.o

kernel/test_node.o: kernel/test_node.c graph/node.h
	$(CC) $(CFLAGS) -c kernel/test_node.c -o kernel/test_node.o

clean:
	rm -f graph/*.o kernel/*.o test_node

.PHONY: clean