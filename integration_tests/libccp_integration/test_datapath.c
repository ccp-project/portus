#include <pthread.h>
#include "stdio.h"
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/un.h>
#include <unistd.h>

#include "libccp/ccp.h"
#include "libccp/serialize.h"

/*
 * Mock datapath for integration tests with userspace CCP in portus.
 * Sets up IPC using unix sockets to /tmp/ccp/0/{in,out}
 * where the portus integration test binary sends a set of complicated events
 * to test the libccp functionality. The mock datapath sets known values for the primitives
 * so the report values can be verified.
 *
 * This binary is called from the portus integraton test program, meant for the integration tests
 * run within the portus testing suites.
 */

#define TO_CCP_SOCKET "/tmp/ccp/0/in"
#define FROM_CCP_SOCKET "/tmp/ccp/0/out"

// global variables
int send_sock = 0; // used for send_msg via ipc
u64 time_zero;
struct ccp_connection* ccp_conn;
double SLEEP_TIME = .001; // in seconds

// get the current time in microseconds
u64 CurrentTime(){
    struct timeval currentTime;
    gettimeofday(&currentTime, NULL);
    return (u64)(currentTime.tv_sec * (int)1e6 + currentTime.tv_usec);
}

struct mock_ccp_state {
    u32 cwnd;
    u32 rate;
};

struct mock_ccp_state mock_conn = {
    .cwnd = 1500, // default value
    .rate = 0
};

/*
 * Mock datapath set cwnd
 */
static void mock_datapath_set_cwnd(__attribute__((unused))struct ccp_datapath *dp, struct ccp_connection *conn, u32 cwnd) {
    struct mock_ccp_state* conn_state = (struct mock_ccp_state*)conn->impl;
    conn_state->cwnd = cwnd;
}

/*
 * Mock datapath set rate rel
 */
static void mock_datapath_set_rate_rel(__attribute__((unused))struct ccp_datapath *dp, struct ccp_connection *conn, u32 rate_factor) {
    struct mock_ccp_state* conn_state = (struct mock_ccp_state*)conn->impl;
    uint32_t new_rate = conn_state->rate * rate_factor;
    if (rate_factor != 0) {
        conn_state->rate = (new_rate/rate_factor);
    }
}

/*
 * Mock datapath set rate abs
 */
static void mock_datapath_set_rate_abs(__attribute__((unused))struct ccp_datapath *dp, struct ccp_connection *conn, u32 rate) {
    struct mock_ccp_state* conn_state = (struct mock_ccp_state*)conn->impl;
    conn_state->rate = rate;
}

/*
 * Mock datapath send msg
 */
static int mock_datapath_send_msg(__attribute__((unused))struct ccp_datapath *dp, __attribute__((unused))struct ccp_connection *conn, char *msg, int msg_size) {
    if (send(send_sock, msg, (uint8_t)msg_size, 0) < 0) {
        printf("Failed to send msg to ccp\n");
        return -1;
    }
    return 0;
}

/*
 * Mock datapath time now function
 */
static u64 mock_datapath_now(void) {
    return CurrentTime() - time_zero;
}

/*
 * Mock datapath since time function
 */
static u64 mock_datapath_since_usecs(u64 then) {
    u64 now = mock_datapath_now();
    return now - then;
}

/*
 * Mock datapath after function
 */
static u64 mock_datapath_after_usecs(u64 usecs) {
    u64 now = mock_datapath_now();
    return now + usecs;
}

/*
 * Fills in the congestion primitives
 */
void fill_in_primitives(int i, struct ccp_connection* ccp_conn) {
    struct mock_ccp_state* conn_state = (struct mock_ccp_state*)ccp_conn->impl;
    ccp_conn->prims.packets_acked = i;
    ccp_conn->prims.bytes_acked = 0;
    ccp_conn->prims.packets_misordered = 0;
    ccp_conn->prims.bytes_misordered = 0;
    ccp_conn->prims.lost_pkts_sample = 0;
    ccp_conn->prims.rtt_sample_us = 2;
    ccp_conn->prims.bytes_acked = 5;
    ccp_conn->prims.packets_misordered = 10;
    ccp_conn->prims.bytes_misordered = 100;
    ccp_conn->prims.lost_pkts_sample = 52;
    ccp_conn->prims.packets_in_flight = 100;
    ccp_conn->prims.rate_outgoing = 2;
    ccp_conn->prims.rate_incoming = 52;
    ccp_conn->prims.snd_cwnd = conn_state->cwnd;
    ccp_conn->prims.snd_rate = conn_state->rate;
    return;
}

/*
 * Switches between reading messages from the ccp module,
 * and calling ccp_invoke to keep the mock datapath state machine going.
 */
void listen_for_messages(int recv_sock) {
    int ok = 0;
    char recvBuf[BIGGEST_MSG_SIZE];
    while (true) {
        int bytes_rcvd = recvfrom(recv_sock, recvBuf, BIGGEST_MSG_SIZE, 0, NULL, NULL);
        if (bytes_rcvd > 0) { 
            ok = ccp_read_msg((char*)recvBuf, bytes_rcvd);
            if (ok < 0) {
                printf("Error reading msg from ccp\n");
            }
        } else {
            fill_in_primitives(52, ccp_conn);
            ccp_invoke(ccp_conn);
            sleep(SLEEP_TIME); // sleep for 1 ms
        }
    } 
    return;
}

/* Listens on tmp/ccp/out for messages from userspace
 */
int setup_listening_thread() {
    
    // setup socket for listening
    int recv_sock = 0;
    struct sockaddr_un recv_sockaddr;
    int path_len = 0;
    struct timeval tv;
    tv.tv_sec = 0;
    tv.tv_usec = 100;

    if ((recv_sock = socket(AF_UNIX, SOCK_DGRAM, 0)) == -1) { 
        printf("Could not setup listening socket\n");
        exit(-1);
    }

    if (setsockopt(recv_sock, SOL_SOCKET, SO_RCVTIMEO,&tv,sizeof(tv)) < 0) {
       printf("Error on setting timeout");
       exit(-1);
    }

    recv_sockaddr.sun_family = AF_UNIX;
    strcpy(recv_sockaddr.sun_path, FROM_CCP_SOCKET);
    unlink(recv_sockaddr.sun_path);
#ifdef __APPLE__
    path_len = SUN_LEN(&recv_sockaddr);
#else
    path_len = strlen(recv_sockaddr.sun_path) + sizeof(recv_sockaddr.sun_family);
#endif
    
    if ((bind(recv_sock, (struct sockaddr*)(&recv_sockaddr), path_len)) < 0) {
        printf("Issue binding to listening socket\n");
        exit(-1);
    }

    return recv_sock;
}

/*
 * Sets up sending side of IPC used for mock datapath.
 * Uses the send_sock descriptor defined globally above.
 */
void setup_send_socket() {
    struct sockaddr_un send_sockaddr;
    int path_len = 0;
    int err = 0;
    if ((send_sock = socket(AF_UNIX, SOCK_DGRAM, 0)) == -1) {
        printf("Could not setup sending socket\n");
        exit(-1);
    }

    send_sockaddr.sun_family = AF_UNIX;
    strcpy(send_sockaddr.sun_path, TO_CCP_SOCKET);

#ifdef __APPLE__    
    path_len = SUN_LEN(&send_sockaddr);
#else
    path_len = strlen(send_sockaddr.sun_path) + sizeof(send_sockaddr.sun_family);
#endif 

    if ( (err = (connect(send_sock, (struct sockaddr*)(&send_sockaddr), path_len))) < 0) {
        perror("connect failed. Error");
        exit(-1);
    }
    unlink(send_sockaddr.sun_path);
}

void setup_ccp_datapath(struct ccp_datapath* dp) {
    dp->set_cwnd = &(mock_datapath_set_cwnd);
    dp->set_rate_abs = &(mock_datapath_set_rate_abs);
    dp->set_rate_rel = &(mock_datapath_set_rate_rel);
    dp->send_msg = &(mock_datapath_send_msg);
    dp->time_zero = time_zero;
    dp->now = &(mock_datapath_now);
    dp->since_usecs = &(mock_datapath_since_usecs);
    dp->after_usecs = &(mock_datapath_after_usecs);

    int ok = ccp_init(dp);
    if (ok < 0) {
        printf("Issue initializing ccp datapath\n");
    }
    return;
}

struct ccp_connection* init_mock_connection() {
    struct ccp_connection* ccp_conn;
    struct ccp_datapath_info dp_info = {
        .init_cwnd = 1500*10,
        .mss = 1500,
        .src_ip = 0,
        .src_port = 1,
        .dst_ip = 3,
        .dst_port = 4
    };
    ccp_conn = ccp_connection_start((void*)(&mock_conn), &dp_info);
    if ( ccp_conn == NULL ) {
        printf("Issue initializing ccp conn\n");
        exit(-1);
    }
    return ccp_conn;

}



/*
 * Mock datapath program: used for portus integration test
 */
int main(__attribute__((unused))int argc, __attribute__((unused))char **argv) {
    struct ccp_datapath dp;
    time_zero = CurrentTime();

    // set up sending socket
    setup_send_socket();

    // register ccp datapath function
    setup_ccp_datapath(&dp);
    
    // setup receiving socket
    int recv_sock = setup_listening_thread();
    
    // initialize a fake connection; sends create messages
    ccp_conn = init_mock_connection();

    // fill in fake values for the primitives
    fill_in_primitives(52, ccp_conn);
   
    // loop on reading for messages, or calling ccp_invoke 
    listen_for_messages(recv_sock);
    
    close(recv_sock);
    close(send_sock);
    return 0;
};
