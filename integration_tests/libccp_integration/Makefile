CC = gcc # C compiler
CFLAGS = -fPIC -Wall -Wextra -O2 -g # C flags
CFLAGS += -D__USRLIB__ # Only use this makefile for compiling user-space library
ifeq ($(DEBUG), 1)
	CFLAGS += -D__DEBUG__
else
endif
LDFLAGS = -shared  # linking flags
RM = rm -f  # rm command
LIB_NAME = ccp
LIBCCP = lib${LIB_NAME}.so # target lib


TEST_DATAPATH = test_datapath
SRCS = libccp/ccp.c libccp/machine.c libccp/serialize.c libccp/ccp_priv.c # source files
OBJS = $(SRCS:.c=.o)

TEST_CCP = ./target/debug/test_ccp

INTEGRATE_SRCS= test_datapath.c
INTEGRATE_OBS = $(INTEGRATE_SRCS:.c=.o)

.PHONY: all
all: ${LIBCCP} ${TEST_DATAPATH} ${TEST_CCP}

$(LIBCCP): $(OBJS)
	$(CC) ${LDFLAGS} -o $@ $^

$(SRCS:.c=.d):%.d:%.c
	$(CC) $(CFLAGS) -MM $< >$@

include $(SRCS:.c=.d)

$(TEST_DATAPATH): ${LIBCCP} ${INTEGRATE_SRCS}
	$(CC) ${CFLAGS} ${INTEGRATE_SRCS} -L . -l ${LIB_NAME} -o ${TEST_DATAPATH}

.PHONY: $(TEST_CCP)
$(TEST_CCP):
	cargo +nightly build

.PHONY: clean
clean:
	-${RM} ${LIBCCP} ${OBJS} $(SRCS:.c=.d) ${TEST_DATAPATH}
	-${RM} -r *.dSYM

