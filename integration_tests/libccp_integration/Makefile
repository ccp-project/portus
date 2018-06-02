CC = gcc # C compiler
CFLAGS = -fPIC -Wall -Wextra -O2 -g # C flags
CFLAGS += -D__USRLIB__ # Only use this makefile for compiling user-space library
ifeq ($(DEBUG), 1)
	CFLAGS += -D__DEBUG__
else
endif
LDFLAGS = -shared  # linking flags
RM = rm -f  # rm command

LIBCCP = libccp.so
TEST_DATAPATH = test_datapath
TEST_CCP = ./target/debug/test_ccp

INTEGRATE_SRCS= test_datapath.c
INTEGRATE_OBS = $(INTEGRATE_SRCS:.c=.o)

.PHONY: all
all: ${LIBCCP} ${TEST_DATAPATH} ${TEST_CCP}

$(LIBCCP):
	$(MAKE) -C libccp DEBUG=n

$(TEST_DATAPATH): ${LIBCCP} ${INTEGRATE_SRCS}
	$(CC) ${CFLAGS} ${INTEGRATE_SRCS} -L ./libccp -lccp -o ${TEST_DATAPATH}

.PHONY: $(TEST_CCP)
$(TEST_CCP):
	cargo +nightly build

.PHONY: clean
clean:
	-${RM} libccp.so ${TEST_DATAPATH}
	-${RM} -r *.dSYM
	-$(MAKE) -C libccp clean

