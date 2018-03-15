/*
 * Character device for IPC between user-space and kernel-space CCP proccesses
 *
 * Frank Cangialosi <frankc@csail.mit.edu>
 * Created: October 17, 2017
 *
 */

#include <linux/atomic.h>
#include <linux/cdev.h>
#include <linux/device.h>
#include <linux/errno.h>
#include <linux/fs.h>
#include <linux/init.h>
#include <linux/kernel.h>
#include <linux/module.h>
#include <linux/moduleparam.h>
#include <linux/mutex.h>
#include <linux/slab.h>
#include <linux/types.h>
#include <linux/uaccess.h>

#include "ccpkp.h"

#define DEV_NAME "ccpkp"

struct ccpkp_dev *ccpkp_dev;
int ccpkp_major;

static struct file_operations ccpkp_fops = 
{
	.owner    = THIS_MODULE,
	.open			= ccpkp_user_open,
	.read			= ccpkp_user_read,
	.write		= ccpkp_user_write,
	.release	= ccpkp_user_release
};

int __init ccpkp_init(void) {
	int result, err;
	int devno;
	dev_t dev = 0;

	result = alloc_chrdev_region(&dev, 0, 1, DEV_NAME);
	ccpkp_major = MAJOR(dev);
	if (result < 0) {
		printk(KERN_WARNING "ccp-kpipe: failed to register\n");
		return result;
	}

	ccpkp_dev = kmalloc(1 * sizeof(struct ccpkp_dev), GFP_KERNEL);
	if (!ccpkp_dev) {
		result = -ENOMEM;
		goto fail;
	}
	memset(ccpkp_dev, 0, 1 * sizeof(struct ccpkp_dev));
	
	mutex_init(&(ccpkp_dev->mux));
	devno = MKDEV(ccpkp_major, 0);
	cdev_init(&ccpkp_dev->cdev, &ccpkp_fops);
	ccpkp_dev->cdev.owner = THIS_MODULE;
	ccpkp_dev->cdev.ops = &ccpkp_fops;
	err = cdev_add(&ccpkp_dev->cdev, devno, 1);
	if (err) {
		printk(KERN_NOTICE "ccp-kpipe: error %d adding cdev\n", err);
	}
	
	printk(KERN_INFO "ccp-kpipe: device (%d) created successfully\n", ccpkp_major);


	return 0;

fail:
	ccpkp_cleanup();
	return result;
}

void ccpkp_cleanup(void) {
	dev_t devno = MKDEV(ccpkp_major, 0);

	if (ccpkp_dev) {
		// TODO free all queue buffers
		cdev_del(&ccpkp_dev->cdev);
		kfree(ccpkp_dev);
	}
	unregister_chrdev_region(devno, 1);
	ccpkp_dev = NULL;

	printk(KERN_INFO "ccp-kpipe: goodbye\n");
}

int ccpkp_user_open(struct inode *inp, struct file *fp) {
	// Create new pipe for this CCP
	struct kpipe *pipe = kmalloc(sizeof(struct kpipe), GFP_KERNEL);
	int i, ccp_id; 

	memset(pipe, 0, sizeof(struct kpipe));
	if (!pipe) {
		return -ENOMEM;
	}
#ifdef ONE_PIPE
	if (!_rb_init(&pipe->kq, true)) {
#else
	if (!_rb_init(&pipe->kq, false)) {
#endif
		return -ENOMEM;
	}
	if (!_rb_init(&pipe->uq, true)) {
		return -ENOMEM;
	}
	
	// Store pointer to pipe in struct file
	fp->private_data = pipe;

	if (mutex_lock_interruptible(&ccpkp_dev->mux)) {
		// We were interrupted (e.g. by a signal),
		// Let the kernel figure out what to do, maybe restart syscall
		return -ERESTARTSYS;
	}
	// TODO this gets decremented later, need to get last allocated instead
	ccp_id = ccpkp_dev->num_ccps;
	if (ccp_id >= MAX_CCPS) {
		ccp_id = -1;
		for (i = 0; i < MAX_CCPS; i++) {
			if (ccpkp_dev->pipes[i] == NULL) {
				ccp_id = i;
				break;
			}
		}
		if (ccp_id == -1) {
			printk(KERN_WARNING "ccp-kpipe: max ccps registered\n");
			return -ENOMEM;
		}
	}
	ccpkp_dev->pipes[ccp_id] = pipe;
	pipe->ccp_id = ccp_id;
	ccpkp_dev->num_ccps++;
	mutex_unlock(&ccpkp_dev->mux);

	return 0;
}

int ccpkp_user_release(struct inode *inp, struct file *fp) {
	struct kpipe *pipe = fp->private_data;
	int ccp_id = pipe->ccp_id;

	if (mutex_lock_interruptible(&ccpkp_dev->mux)) {
		return -ERESTARTSYS;
	}
	ccpkp_dev->pipes[pipe->ccp_id] = NULL;
	ccpkp_dev->num_ccps--;
	mutex_unlock(&ccpkp_dev->mux);
	
	kpipe_cleanup(pipe);
	fp->private_data = NULL;

	printk(KERN_INFO "ccp-kpipe: ccp %d closed\n", ccp_id);
	return 0;
}

ssize_t ccpkp_user_read(struct file *fp, char *buf, size_t bytes_to_read, loff_t *offset) {
	struct kpipe *pipe = fp->private_data;
#ifdef ONE_PIPE
	struct ringbuf *q = &(pipe->kq);
#else
	struct ringbuf *q = &(pipe->uq);
#endif
	return kp_read(pipe, q, buf, bytes_to_read, true);
}

// module stores pointer to corresponding ccp kpipe for each socket
ssize_t ccpkp_kernel_read(struct kpipe *pipe, char *buf, size_t bytes_to_read) {
	struct ringbuf *q = &(pipe->kq);
	return kp_read(pipe, q, buf, bytes_to_read, false);
}

//ssize_t ccpkp_user_read(struct file *fp, char *buf, size_t bytes_to_read, loff_t *offset) {
ssize_t kp_read(struct kpipe *pipe, struct ringbuf *q, char *buf, size_t bytes_to_read, bool user_buf) {
	size_t bytes_read = 0;
	//uint8_t *pkt_len;
	char *safe_wp = q->buf + q->wp;


	PDEBUG("READ_START: rp=%p wp=%p, bytes_to_read=%lu\n", q->rp, safe_wp, bytes_to_read);

	// Kernel doesn't wait if pipe is empty
	if (!user_buf && (safe_wp == q->rp)) {
		return 0;
	}

	while ((q->buf + q->wp) == q->rp) { // TODO optimistic spinlock
		PDEBUG("pipe empty, sleeping...\n");
		if (wait_event_interruptible(q->nonempty, (q->buf + q->wp) != q->rp)) {
			return -ERESTARTSYS;
		}
	}
	// Copy of write pointer at this time 
	// Can't keep accessing q->wp because writer might be writing now
	safe_wp = q->buf + q->wp;

	// for now, reader only reads one packet at a time
	// (1) ensure there is enough space in the user buffer for a single packet
	/*
	pkt_len = ((uint8_t *)safe_wp)+1;
	if (bytes_to_read < *pkt_len) {
		PDEBUG("not enough space in buf for msg\n");
		return -EFAULT;
	}
	if (*pkt_len > BIGGEST_MSG_SIZE) {
		PDEBUG("corrupted msg! pkt_len=%d > %d", *pkt_len, BIGGEST_MSG_SIZE);
		return -EFAULT;
	}
	bytes_to_read = (size_t)*pkt_len;
	*/
	
	if (safe_wp > q->rp) { // No wraparound
		bytes_read = min(bytes_to_read, (size_t)(safe_wp - q->rp));
	} else { // Wraparound, first read from rp to end
		bytes_read = min(bytes_to_read, (size_t)(q->end - q->rp));
	}
	PDEBUG("reading %li bytes\n", (long)bytes_read);
	if (user_buf) {
		if (copy_to_user(buf, q->rp, bytes_read)) {
			return -EFAULT;
		}
	} else {
		memcpy(buf, q->rp, bytes_read);
	}
	q->rp += bytes_read;
	if (q->rp == q->end) { // We reached to the end, wrap
		PDEBUG("read pointer wrapped\n");
		q->rp = q->buf;
		// If we wanted to read more data and the buffer wrapped, read the rest
		if (bytes_read < bytes_to_read) {
			safe_wp = q->buf + q->wp;
			if (safe_wp > q->rp) {
				bytes_to_read = min(bytes_to_read - bytes_read, (size_t)(safe_wp - q->rp));
				PDEBUG("reading %li more bytes\n", (long)bytes_to_read);
				if (user_buf) {
					if (copy_to_user(buf + bytes_read, q->rp, bytes_to_read)) {
						return -EFAULT;
					}
				} else {
					memcpy(buf + bytes_read, q->rp, bytes_to_read);
				}
				bytes_read += bytes_to_read;
				q->rp += bytes_to_read;
			}
		}
	}

	PDEBUG("user read %li bytes\n", (long) bytes_read);
	PDEBUG("READ_END: rp=%p wp=%p\n", q->rp, q->buf+q->wp);
	return bytes_read;
}

ssize_t ccpkp_user_write(struct file *fp, const char *buf, size_t bytes_to_write, loff_t *offset) {
	struct kpipe *pipe = fp->private_data;
	struct ringbuf *q = &(pipe->kq);
#ifdef MULTI
	return kp_write_multi(pipe, q, buf, bytes_to_write, true);
#else
	return kp_write_single(pipe, q, buf, bytes_to_write, true);
#endif
}

// module stores pointer to corresponding ccp kpipe for each socket
ssize_t ccpkp_kernel_write(struct kpipe *pipe, const char *buf, size_t bytes_to_write) {
	struct ringbuf *q = &(pipe->uq);
#ifdef MULTI
	return kp_write_multi(pipe, q, buf, bytes_to_write, false);
#else
	return kp_write_single(pipe, q, buf, bytes_to_write, false);
#endif
}

ssize_t kp_write_multi(struct kpipe *pipe, struct ringbuf *q, const char *buf, size_t bytes_to_write, bool user_buf) {
	size_t bytes_wrote = 0;
	char *safe_rp = q->rp;
	int read_offset = (int)(safe_rp - q->buf);
	int old_wp_tmp, new_wp_tmp;
	int avail = 0;
	char *safe_wp;
	int old_wp, new_wp;
	int old_cs, old_cb, old_ce;

	PDEBUG("write start");
	
	// Reserve chunk of ringbuffer (old_wp_tmp, new_wp_tmp) 
	// by atomically incrementing wp_tmp
	do {
		old_wp_tmp = q->wp_tmp;
		new_wp_tmp = (old_wp_tmp + bytes_to_write) % PER_Q_BSIZE;
		avail = (PER_Q_BSIZE - (old_wp_tmp - read_offset) - 1) % PER_Q_BSIZE;
		if (bytes_to_write > avail) {
			PDEBUG("not enough space in buffer (read=%d, write=%d, want=%lu)\n", read_offset, old_wp_tmp, bytes_to_write);
			return -EAGAIN;
		}
	} while (cmpxchg(&(q->wp_tmp), old_wp_tmp, new_wp_tmp) != old_wp_tmp);
	PDEBUG("acquired chunk [%d, %d]", old_wp_tmp, new_wp_tmp);

	safe_wp = q->buf + old_wp_tmp;


	// Write bytes_to_write bytes from buf into q from old to new
	if(safe_wp >= safe_rp) {
		bytes_wrote = min(bytes_to_write, (size_t)(q->end - safe_wp));
	} else {
		bytes_wrote = min(bytes_to_write, (size_t)(safe_rp - safe_wp - 1));
	}
	PDEBUG("going to write %li bytes\n", (long)bytes_wrote);
	if (copy_from_user(safe_wp, buf, bytes_wrote)) {
		return -EFAULT;
	}
	safe_wp += bytes_wrote;
	if (safe_wp == q->end) {
		safe_wp = q->buf;
	}
	//wake_up_interruptible(&q->nonempty);
	if (safe_wp == q->buf) { 
		if (bytes_wrote < bytes_to_write) {
			safe_rp = q->rp;
			if (safe_rp > safe_wp) {
				bytes_to_write = min(bytes_to_write - bytes_wrote, (size_t)(safe_rp - safe_wp  - 1));
				PDEBUG("going to write %li more bytes\n", (long)bytes_to_write);
				if (copy_from_user(safe_wp, buf + bytes_wrote, bytes_to_write)) {
					return -EFAULT;
				}
				bytes_wrote += bytes_to_write;
				safe_wp += bytes_to_write;
			}
		}
	}

	// Make sure we used exactly the amount of space we reserved
	WARN_ON(safe_wp != (q->buf + new_wp_tmp));

	// Shift the actual write pointer appropriately so readers can read it
	do {
		old_wp = q->wp;
		new_wp = new_wp_tmp;

		old_cs = q->chunk_size;
		old_cb = q->chunk_begin;
		old_ce = q->chunk_end;
		if (old_wp != old_wp_tmp) { 
			// This means there's a pending write before me
			// Want to update chunk
			q->chunk_begin = old_wp_tmp < old_wp ? min(old_cb, old_wp_tmp) : max(old_cb, old_wp_tmp);
			q->chunk_end = old_wp_tmp < old_wp ? max(old_ce, new_wp_tmp) : min(old_ce, new_wp_tmp);
			if (cmpxchg(&(q->chunk_size), old_cs, old_cs + bytes_wrote) != old_cs) {
				continue;
			} else {
			// But not update wp
				break;
			}
		} else {
			if (new_wp_tmp == old_cb && ((old_ce - old_cb) == old_cs)) { 
				// We've written up to the chunk, and the chunk has no holes, so we can
				// commit all of it
				if (cmpxchg(&(q->chunk_size), old_cs, 0) != old_cs) {
					continue;
				}
				q->chunk_size = 0;
				q->chunk_begin = new_wp_tmp;
				q->chunk_end = new_wp_tmp;
				new_wp = old_ce;
			}
		}
	} while (cmpxchg(&(q->wp), old_wp, new_wp) != old_wp);

	PDEBUG("shifted wp from %d to %d", old_wp, new_wp);
	
	PDEBUG("write end");
	return bytes_wrote;
}

ssize_t kp_write_single(struct kpipe *pipe, struct ringbuf *q, const char *buf, size_t bytes_to_write, bool user_buf) {
	size_t bytes_available = 0,
				 bytes_wrote = 0;
	char *safe_rp = q->rp;
	char *wp = (q->buf + q->wp);


	PDEBUG("WRITE_START: rp=%p wp=%p\n", q->rp, wp);
	if (safe_rp == wp) {
		bytes_available = PER_Q_BSIZE - 1;
	} else {
		bytes_available = ((safe_rp - wp + PER_Q_BSIZE) % PER_Q_BSIZE) - 1;
	}

	if (bytes_available <= 0 || bytes_available < bytes_to_write) {
		PDEBUG("not enough space in buffer (%li remaining), not waiting\n", (long)bytes_available);
		return -EAGAIN;
	}

	if(wp >= safe_rp) {
		bytes_wrote = min(bytes_to_write, (size_t)(q->end - wp));
	} else {
		bytes_wrote = min(bytes_to_write, (size_t)(safe_rp - wp - 1));
	}
	PDEBUG("going to write %li bytes\n", (long)bytes_wrote);
	if (copy_from_user(wp, buf, bytes_wrote)) {
		return -EFAULT;
	}
	wp += bytes_wrote;
	if (wp == q->end) {
		wp = q->buf;
	}
	if (wp == q->buf) { 
		if (bytes_wrote < bytes_to_write) {
			safe_rp = q->rp;
			if (safe_rp > wp) {
				bytes_to_write = min(bytes_to_write - bytes_wrote, (size_t)(safe_rp - q->wp  - 1));
				PDEBUG("going to write %li more bytes\n", (long)bytes_to_write);
				if (copy_from_user(wp, buf + bytes_wrote, bytes_to_write)) {
					return -EFAULT;
				}
				bytes_wrote += bytes_to_write;
				wp += bytes_to_write;
			}
		}
	}
	q->wp = (wp - q->buf);
#ifdef ONE_PIPE
	PDEBUG("waking readers...");
	wake_up_interruptible(&q->nonempty);
#endif

	PDEBUG("wrote %li bytes total\n", (long)bytes_wrote);
	PDEBUG("WRITE_END: rp=%p wp=%p\n", q->rp, wp);
	return bytes_wrote;
}

module_init(ccpkp_init);
module_exit(ccpkp_cleanup);

MODULE_LICENSE("GPL");
MODULE_AUTHOR("Frank Cangialosi <frankc@csail.mit.edu>");
MODULE_DESCRIPTION("Character device for IPC between user and kernel CCP");
MODULE_VERSION("0.1");
