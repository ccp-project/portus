TARGET = nltest
test-objs := netlink.o

obj-m := $(TARGET).o

all: nltest.ko

nltest.ko:
	make -C /lib/modules/$(shell uname -r)/build M=$(shell pwd) modules

clean:
	make -C /lib/modules/$(shell uname -r)/build M=$(shell pwd) clean
