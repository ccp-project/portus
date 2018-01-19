# Comment/uncomment the following line to disable/enable debugging
DEBUG = n
ONE_PIPE = y
MULTI = n

# Add your debugging flag (or not) to EXTRA_CFLAGS
ifeq ($(DEBUG),y)
  DEBFLAGS = -O1 -g -DDEBUG_MODE # "-O" is needed to expand inlines
else
  DEBFLAGS = -Ofast
endif

ifeq ($(ONE_PIPE),y)
	DEBFLAGS += -DONE_PIPE
endif

ifeq ($(MULTI),y)
	DEBFLAGS += -DMULTI
endif

EXTRA_CFLAGS += $(DEBFLAGS)
EXTRA_CFLAGS += -I$(LDDINC)

ifneq ($(KERNELRELEASE),)
# call from kernel build system

obj-m	:= ccpkp.o

else

KERNELDIR ?= /lib/modules/$(shell uname -r)/build
PWD       := $(shell pwd)

modules:
	$(MAKE) -C $(KERNELDIR) M=$(PWD) LDDINC=$(PWD)/../include modules

endif

manual-test:
	$(CC) manual-test.c -o manual-test

clean:
	rm -rf *.o *~ core .depend .*.cmd *.ko *.mod.c .tmp_versions manual-test

depend .depend dep:
	$(CC) $(EXTRA_CFLAGS) -M *.c > .depend

ifeq (.depend,$(wildcard .depend))
include .depend
endif
