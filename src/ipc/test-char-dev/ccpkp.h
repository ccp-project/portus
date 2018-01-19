#ifndef _CCPKP_H_
#define _CCPKP_H_

#include <linux/slab.h>

#undef PDEBUG             /* undef it, just in case */
#ifdef DEBUG_MODE
#  ifdef __KERNEL__
     /* This one if debugging is on, and kernel space */
#    define PDEBUG(fmt, args...) printk( KERN_DEBUG "ccp-kpipe: " fmt, ## args)
#  else
     /* This one for user space */
#    define PDEBUG(fmt, args...) fprintf(stderr, fmt, ## args)
#  endif
#else
#  define PDEBUG(fmt, args...) /* not debugging: nothing */
#endif

#ifndef PER_Q_BSIZE
#define PER_Q_BSIZE 4000
#endif

#ifndef MAX_CCPS
#define MAX_CCPS 32
#endif

#define BIGGEST_MSG_SIZE 256

struct ringbuf {
	wait_queue_head_t nonempty;
	char *buf, *end;
	char *rp;
	int wp,
			wp_tmp;

	int chunk_begin,
			chunk_size,
			chunk_end;
};

static __always_inline bool _rb_init(struct ringbuf *rb, bool blocking) {
	rb->buf = kmalloc(PER_Q_BSIZE, GFP_KERNEL);
	if (!rb->buf) {
		return false;
	}
	rb->end = rb->buf + PER_Q_BSIZE;
	rb->rp = rb->buf;
	if (blocking) {
		init_waitqueue_head(&(rb->nonempty));
	}
	
	rb->wp = rb->wp_tmp = 0;
	rb->chunk_begin = rb->chunk_end = rb->chunk_size = 0;

	return true;
}

struct kpipe {
	int ccp_id; 	 /* Index of this pipe pipe list */
	struct ringbuf kq; /* Queue from user to kernel */
	struct ringbuf uq; /* Queue from kernel to user */
};

void kpipe_cleanup(struct kpipe *pipe) {
	kfree(pipe->kq.buf);
	kfree(pipe->uq.buf);
	kfree(pipe);
}



struct ccpkp_dev {
	int num_ccps;
	struct kpipe *pipes[MAX_CCPS];
	struct cdev cdev;
	struct mutex mux;
};

int __init  ccpkp_init(void);
int		 	    ccpkp_user_open(struct inode *, struct file *);
ssize_t			ccpkp_user_read(struct file *fp, char *buf, size_t bytes_to_read, loff_t *offset);
ssize_t			ccpkp_kernel_read(struct kpipe *pipe, char *buf, size_t bytes_to_read);
ssize_t			kp_read(struct kpipe *pipe, struct ringbuf *q, char *buf, size_t bytes_to_read, bool user_buf);
ssize_t			ccpkp_user_write(struct file *fp, const char *buf, size_t bytes_to_write, loff_t *offset);
ssize_t			ccpkp_kernel_write(struct kpipe *pipe, const char *buf, size_t bytes_to_read);
ssize_t			kp_write_multi(struct kpipe *pipe, struct ringbuf *q, const char *buf, size_t bytes_to_write, bool user_buf);
ssize_t			kp_write_single(struct kpipe *pipe, struct ringbuf *q, const char *buf, size_t bytes_to_write, bool user_buf);
int			    ccpkp_user_release(struct inode *, struct file *);
void        ccpkp_cleanup(void);


#endif
