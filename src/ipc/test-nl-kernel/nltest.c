#include <linux/module.h>
#include <linux/netlink.h>
#include <linux/skbuff.h>
#include <linux/gfp.h>
#include <linux/kprobes.h>
#include <linux/ptrace.h>
#include <net/sock.h>

#define MYMGRP 22

struct sock *nl_sk = NULL;

/* Send message to userspace
 */
int nl_send_msg(unsigned long data, char *payload, size_t msg_size) {
    struct sk_buff *skb_out;
    struct nlmsghdr *nlh;
    int res;


    skb_out = nlmsg_new(
        NLMSG_ALIGN(msg_size), // @payload: size of the message payload
        GFP_NOWAIT             // @flags: the type of memory to allocate.
    );
    if (!skb_out) {
        printk(KERN_ERR "nltest: Failed to allocate new skb\n");
        return -20;
    }

    nlh = nlmsg_put(
        skb_out,    // @skb: socket buffer to store message in
        0,          // @portid: netlink PORTID of requesting application
        0,          // @seq: sequence number of message
        NLMSG_DONE, // @type: message type
        msg_size,   // @payload: length of message payload
        0           // @flags: message flags
    );

    memcpy(nlmsg_data(nlh), payload, msg_size);
    res = nlmsg_multicast(
            nl_sk,     // @sk: netlink socket to spread messages to
            skb_out,   // @skb: netlink message as socket buffer
            0,         // @portid: own netlink portid to avoid sending to yourself
            MYMGRP,    // @group: multicast group id
            GFP_NOWAIT // @flags: allocation flags
    );

    return res;
}

/* Receive echo message from userspace 
 * Respond echo it back for checking
 */
void nl_recv_msg(struct sk_buff *skb) {
    struct nlmsghdr *nlh = nlmsg_hdr(skb);
    int res = nl_send_msg(0, (char*) nlmsg_data(nlh), nlh->nlmsg_len - sizeof(struct nlmsghdr));
    if (res < 0) {
        pr_info("nltest: echo send failed: %d\n", res);
    }
    printk(KERN_INFO "nltest: Echoed message len %u\n", nlh->nlmsg_len);
}

static int __init nl_init(void) {
    struct netlink_kernel_cfg cfg = {
           .input = nl_recv_msg,
    };
    
    nl_sk = netlink_kernel_create(&init_net, NETLINK_USERSOCK, &cfg);
    if (!nl_sk) {
        printk(KERN_ALERT "nltest: Error creating socket.\n");
        return -10;
    }

    return 0;
}

static void __exit nl_exit(void) {
    netlink_kernel_release(nl_sk);
}

module_init(nl_init);
module_exit(nl_exit);

MODULE_LICENSE("GPL");
