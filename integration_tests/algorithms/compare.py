#!/usr/bin/env python
import sys
import os
import os.path as path
import subprocess
import argparse
import re
import signal
from time import sleep, strftime
import shutil
from pprint import pprint
import numpy as np
import matplotlib as mpl
mpl.use('Agg')
import matplotlib.pyplot as plt
import matplotlib.gridspec as gridspec
from matplotlib import rc
from collections import OrderedDict
plt.rcParams.update(plt.rcParamsDefault)
plt.style.use('seaborn-white')
mpl.rcParams['xtick.labelsize'] = 12
mpl.rcParams['ytick.labelsize'] = 12
mpl.rcParams['axes.labelsize'] = 14

################################################################################
# The following variables must be set in order to run the integration test
CCP_KERNEL_PATH = None
CCP_KERNEL_PATH = os.getenv('CCP_KERNEL_PATH') or CCP_KERNEL_PATH
PORTUS_PATH     = os.path.abspath(os.path.join(os.path.dirname(os.path.realpath(__file__)), os.pardir))
TEST_DIR        = path.join(PORTUS_PATH, "integration-tests")
LINK_DIR        = path.join(TEST_DIR, "link-traces")
################################################################################
supported_datapaths = ['kernel']
length = 60

################################################################################
# Helpers
################################################################################

class Scenario(object):
    def __init__(self, alg, bw, rtt, bdp, scenario_num):
        self.alg = alg
        self.bw = bw
        self.rtt = rtt
        self.bdp = bdp
        self.num = scenario_num

    def __str__(self):
        return "Scenario #{}: {}, {}bps, {}ms RTT, {}BDP buffer".format(
            self.num,
            self.alg,
            self.bw,
            self.rtt,
            self.bdp
        )

class ExpData(object):
    def __init__(self, loc, lc=None, lt='-', key=None):
        self.loc = loc
        self.lc = lc
        self.lt = lt
        self.key = key

class ParsedMMLog(object):
    def __init__(self, time_vals, tpt_vals, del_vals, duration=None, capacity=None, ingress=None,
            throughput=None, util=None, avg_delay=None, med_delay=None, upper_delay=None):
        self.time_vals = time_vals
        self.tpt_vals = tpt_vals
        self.del_vals = del_vals
        self.duration = duration
        self.capacity = capacity
        self.ingress = ingress
        self.throughput = throughput
        self.util = util
        self.avg_delay = avg_delay
        self.med_delay = med_delay
        self.upper_delay = upper_delay

# Run args as a shell-command and return $? 
def check_return_code(args):
    try:
        subprocess.check_output(args, shell=True)
        return 0
    except subprocess.CalledProcessError as e:
        return e.returncode

# Returns result of "which {prog}" (false if does not exist) 
def binary_exists(prog):
    return check_return_code("which {}".format(prog))

def ask_yes_or_no(question):
    responses = ['y','n']
    resp = ""
    while not resp.lower() in responses:
        resp = raw_input(question + " (y/n) ") 
    return resp == "y"

def working_directory_clean(gitdir):
    return check_return_code("git -C {} diff-index --quiet HEAD --".format(gitdir)) == 0 

def get_current_hash(gitdir):
    return subprocess.check_output(
            "git -C {} rev-parse HEAD".format(gitdir),
            shell=True).strip()[:6]

def enable_ip_forwarding():
    ret = subprocess.check_output("cat /proc/sys/net/ipv4/ip_forward",shell=True)
    ret = ret.strip().replace(" ","").lower()
    if ret != "1":
        ret = subprocess.check_output("exec sudo sysctl -w net.ipv4.ip_forward=1",shell=True)
        ret = ret.strip().lower()
        if ret != "net.ipv4.ip_forward = 1":
            sys.exit("error: unable to enable ip_forwarding, which is required "
                     "to run mahimahi.\ntry: sudo sysctl -w net.ipv4.ip_forward=1")


def mm_shell(link_trace, bw, one_way, bdp, log_dir, prog):
    bdp_bytes = (((bw * 1000000.0) / 8.0) * ((one_way * 2.0) / 1000.0))
    
    cmd = ("mm-link --uplink-log=\"{log_dir}/uplink.log\" "
           "--uplink-queue=\"droptail\" "
           "--downlink-queue=\"droptail\" "
           "--uplink-queue-args=\"bytes={bdp}\" "
           "--downlink-queue-args=\"bytes={bdp}\" "
           "{link_trace} {link_trace} "
           "mm-delay {one_way} {prog}").format(
        link_trace = link_trace,
        one_way=one_way,
        bdp=int(bdp_bytes * bdp),
        log_dir=log_dir,
        prog=prog
    )
    print "(mahimahi: {})".format(cmd)
    return subprocess.Popen("exec " + cmd, shell=True)

def pkill(procnames):
    if not isinstance(procnames, list):
        procnames = [procnames]
    for procname in procnames:
        proc = subprocess.Popen("exec sudo pkill -9 {}".format(procname),
                shell=True)
        proc.wait()

def kill_children():
    subprocess.Popen("exec sudo pkill -P $$", shell=True)

def send_signal(procs, sig):
    if not isinstance(procs, list):
        procs = [procs]
    for proc in procs:
        subprocess.Popen("exec sudo pkill -s {} {} ".format(proc.pid, sig),
                shell=True).wait()
        #proc.send_signal(sig)

def start_portus(alg, ipc, output_dir):
    generic_algs = ['reno', 'cubic']
    included_algs = ['bbr']
    if alg in generic_algs:
        path_fmt = "{portus}/ccp_generic_cong_avoid/target/debug/{alg}"
    elif alg in included_algs:
        path_fmt = "{portus}/ccp_{alg}/target/debug/{alg}"
    else:
        sys.exit("unknown algorithm '{alg}'".format(alg=alg))

    path = path_fmt.format(portus=PORTUS_PATH, alg=alg)
    print "(portus: {})".format(path)
    return (
        subprocess.Popen(("exec sudo {path}"
        " --ipc {ipc} > {out}/portus.out 2>&1").format(
            path=path,
            ipc=ipc,
            out=output_dir
        ), shell=True)
    )
def start_iperf_server(port, output_dir):
    return (
        subprocess.Popen("exec iperf -s -p {port} -f m > {out}/recv.out 2>&1".format(
            out=output_dir,
            port=port
        ), shell=True)
    )

def start_tcpprobe(output_dir):
    return (
        subprocess.Popen("exec cat /proc/net/tcpprobe > {out}/probe.out 2>&1".format(
            out=output_dir
        ), shell=True)
    )

def prepare_tcpprobe():
    if not os.path.isfile('/proc/net/tcpprobe'):
        print "info: tcpprobe not found, loading..."
        subprocess.Popen("exec sudo modprobe tcp_probe port=4242", shell=True).wait()
        subprocess.Popen("exec sudo chmod 444 /proc/net/tcpprobe", shell=True).wait()
        if os.path.isfile('/proc/net/tcpprobe'):
            print "info: tcpprobe loaded successfully!"
        else:
            sys.exit("error: failed to load tcpprobe")

def can_sudo():
    return "sudo" in subprocess.check_output("id", shell=True).split("groups=")[1]

def read_tcpprobe(fname):
    fields = [0, 6]
    with open(fname) as f:
        for l in f:
            if "1480" in l:
                continue
            sp = l.strip().split(' ')
            yield tuple([float(sp[c]) for c in fields])

def parse_mm_log(fname, bin_size):
    proc = subprocess.Popen("exec {}/mm-graph --fake {} {}".format(TEST_DIR, fname, bin_size),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            shell=True)
    proc.wait()
    stdout, stderr = proc.communicate()
    stdout = stdout.split("\n")
    stderr = stderr.split("\n")
    time_data, tpt_data, del_data = [], [], []
    for line in stdout:
        line = line.strip()
        if line != "":
            t, tpt, delay = line.split(" ")
            time_data.append(float(t))
            tpt_data.append(float(tpt))
            del_data.append(float(delay))
    duration = float(stderr[0].split(" ")[1])
    capacity = float(stderr[1].split(" ")[2])
    ingress = float(stderr[2].split(" ")[2])
    throughput = float(stderr[3].split(" ")[2])
    util = stderr[3].split(" ")[4].replace("(","")
    delays = [float(x) for x in stderr[4].split(" ")[5].split("/")]

    return ParsedMMLog(time_data, tpt_data, del_data, duration=duration, capacity=capacity, ingress=ingress,
            throughput=throughput, util=util, avg_delay=delays[0],
            med_delay=delays[1], upper_delay=delays[2])

def find_test_scenarios(parent_dir):
    scenes = {}
    scenario_num = 1
    for d in os.listdir(parent_dir):
        if os.path.isdir(path.join(parent_dir, d)):
            ret = re.match("([a-zA-Z]+)\.([0-9kmKM]+)\.([0-9]+)ms\.([0-9]+)bdp", d)
            if ret:
                alg, bw, rtt, bdp = ret.groups()
            else:
                sys.exit("error: invalid reference scenario directory "
                "format, expected {{algorithm}}.{{bw}}m.{{rtt}}ms.{{bdp}}bdp")
            rtt, bdp = int(rtt), int(bdp)

            scenario = Scenario(alg, bw, rtt, bdp, scenario_num)
            scenes[d] = scenario
            scenario_num += 1
    return scenes


def run_scenario(scenario, ipc, parent_dir):
    link_trace = os.path.join(LINK_DIR, scenario.bw.lower() + ".mahi")
    if not os.path.isfile(link_trace):
        sys.exit("""error: file not found: {link_trace}

In order to run a test at {bw}, you must create a corresponding trace file in
{link_dir}""".format(bw=scenario.bw, link_dir=LINK_DIR, link_trace=link_trace))

    portus_proc = start_portus(scenario.alg, ipc, parent_dir)
    recv_proc = start_iperf_server(4242, parent_dir)
    probe_proc = start_tcpprobe(parent_dir)

    bw = int(scenario.bw[:-1])
    mm_proc = mm_shell(link_trace, bw, int(scenario.rtt / 2), scenario.bdp, parent_dir,
        "python {}/compare-inner.py {} {}".format(
            TEST_DIR,
            length,
            parent_dir
        )
    )

    mm_proc.wait()

    # Try to kill everyone nicely
    sleep(0.5)
    send_signal([portus_proc, recv_proc, probe_proc], signal.SIGINT)
    sleep(0.5)
    # Make triple sure everyone is dead
    kill_children()
    pkill([scenario.alg, 'iperf', 'cat', 'mm-link', 'mm-delay'])
    send_signal([portus_proc, recv_proc, probe_proc], signal.SIGKILL)
################################################################################




if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="test to compare behavior "
    "between different versions of portus+datapath")
    parser.add_argument('mode', type=str, help="reference-trace | compare-commits | new-reference")
    parser.add_argument('commits', nargs='*', help="commits to compare")
    parser.add_argument('--datapath',
            help="backend datapath to test (kernel), default=kernel",
            default="kernel")
    parser.add_argument('--allow-dirty', action="store_true", help="allow testing with dirty working tree")
    parser.add_argument('--overwrite', action="store_true", help="always overwrite existing test results")
    parser.add_argument('--replot', action="store_true", help="don't rerun tests, just replot the existing results")
    parser.add_argument('--downsample', type=int, default=0, help="downsample datapoints by this factor")

    args = parser.parse_args()

    if not can_sudo():
        sys.exit("error: your user needs sudo to run portus")

    if not args.datapath in supported_datapaths:
        sys.exit("error: {} datapath not yet supported.".format(args.datapath))

    # TODO add others later
    if args.datapath == "kernel":
        datapath_module = "ccp-kernel"
        ipc = "netlink"
        if CCP_KERNEL_PATH is None:
            sys.exit("The kernel datapath requires the ccp-kernel repo. Please use"
            " the full *absolute* path to either\n(1) Update CCP_KERNEL_PATH at"
            " the top of ./integration-test/compare.py -or-\n(2) Set the"
            " CCP_KERNEL_PATH environment variable (e.g. export"
            " CCP_KERNEL_PATH=...)")


    if not args.allow_dirty:
        #if not working_directory_clean(CCP_KERNEL_PATH):
        #    sys.exit('error: there are unstashed or uncommited changes in '
        #    'ccp-kernel; the repository must be clean to test against a different '
        #    'commit hash (or re-run test with --allow-dirty)')
        #if not working_directory_clean(PORTUS_PATH):
        #    sys.exit('error: there are unstashed or uncommited changes in '
        #    'portus; the repository must be clean to test against a different '
        #    'commit hash (or re-run test with --allow-dirty')
        datapath_commit = get_current_hash(CCP_KERNEL_PATH)
        portus_commit = get_current_hash(PORTUS_PATH)
    else:
        if not working_directory_clean(CCP_KERNEL_PATH):
            datapath_commit = "current"
        else:
            datapath_commit = get_current_hash(CCP_KERNEL_PATH)
        if not working_directory_clean(PORTUS_PATH):
            portus_commit = "current"
        else:
            portus_commit = get_current_hash(PORTUS_PATH)

    prepare_tcpprobe()
    enable_ip_forwarding()

    to_compare = {}
    colors = {}
    should_plot = False
    ref_dir = path.join(TEST_DIR, 'reference')


    if args.mode == 'reference-trace':
        if not os.path.isdir(ref_dir):
            sys.exit("error: directory not found: ./integration-tests/reference.\n"
            "Reference-trace mode expects a directory of reference traces at "
            "this location to compare against. New traces can be created with"
            " 'new-reference' mode.")

        print "comparing (portus@{}, {}@{}) to reference traces".format(
            portus_commit,
            datapath_module,
            datapath_commit
        )
        should_plot = True
        ref_scenarios = find_test_scenarios(ref_dir)
        n_scenarios_found = len(ref_scenarios.keys())

        if n_scenarios_found < 1:
            sys.exit("error: ./integration-tests/reference is empty. Must have "
            "at least one reference trace to compare.")

        for d, scenario in sorted(ref_scenarios.iteritems(), key=lambda (k,v): v.num):

            print ("\nTest Scenario #{}/{}: {}, {}bps, {}ms RTT, {} BDP buffer "
            "buffer ({} seconds)").format(scenario.num, n_scenarios_found, scenario.alg,
                    scenario.bw, scenario.rtt, scenario.bdp, length)

            output_dir = path.join(TEST_DIR, "tmp", d, "portus@{}.{}@{}".format(
                portus_commit,
                datapath_module,
                datapath_commit
            ))

            to_compare[scenario] = [
                ExpData(path.join(ref_dir, d), key='Reference'),
                ExpData(output_dir, key='Current')
            ]
            colors['Reference'] = 'C0'
            colors['Current'] = 'C3'

            if args.replot:
                continue

            if os.path.isdir(output_dir):
                if not args.overwrite:
                    resp = ask_yes_or_no("Found previous results for this test, do you want to overwrite them?")
                    if not resp:
                        print "Ok, skipping..."
                        continue
                shutil.rmtree(output_dir)

            os.makedirs(output_dir)

            run_scenario(scenario, ipc, output_dir)

    elif args.mode == 'compare-commits':
        print "compare commits code with " + str(args.commits)
        should_plot = True
    elif args.mode == 'new-reference':
        if not os.path.isdir(ref_dir):
            sys.exit("""error: directory not found: ./integration-tests/reference/.

new-reference mode expects a directory at this location containing
sub-directories of the format {{alg}}.{{mbps}}m.{{rtt}}ms.{{bdp}}bdp, which
specify the link conditions for testing a given algorithm. For example, to
create a trace of reno over a 12Mbps link, with 100ms RTT and 1BDP droptail
buffer, create ./integration-tests/reference/reno.12m.100ms.1bdp.""")

        ref_scenarios = find_test_scenarios(ref_dir)
        n_scenarios_found = len(ref_scenarios.keys())
        if n_scenarios_found < 1:
            sys.exit("error: ./integration-tests/reference is empty. Must "
            "specify at least one scenario to create a reference.")

        for d, scenario in ref_scenarios.iteritems():

            print ("\nFound scenario: {}, {}bps, {}ms RTT, {}BDP buffer ").format(
                    scenario.alg, scenario.bw, scenario.rtt, scenario.bdp)

            resp = ask_yes_or_no("Would you like to re-run this reference trace?")

            if resp:
                print "Running ({} seconds)".format(length)
                run_scenario(scenario, ipc, path.join(ref_dir, d))
            else:
                print "Ok, skipping..."


    else:
        sys.exit("error: unknown mode {}; available options are reference-trace "
        "or compare-commits")

    if should_plot:

        # All measurements in inches
        width = 12
        header = 3.0
        inches_per_scene = 10
        total_height = header + (inches_per_scene * len(to_compare.keys()))
        in_from_top = lambda x : ((total_height - x) / total_height)

        fig = plt.figure(figsize=(width, total_height))
        #fig.suptitle('Portus Integration Test', fontsize=18, fontweight='bold')
        plt.figtext(0.5, in_from_top(0.5), 'Portus Integration Test',
                fontsize=18,
                fontweight='bold',
                ha='center'
        )
        plt.figtext(0.5, in_from_top(0.9) , str(strftime('%c')),
                fontsize=14,
                ha='center'
        )
        plt.figtext(0.5, in_from_top(1.2), 'portus@{}, {}@{}'.format(
                portus_commit,
                datapath_module,
                datapath_commit
        ), fontsize=14, ha='center')

        fig.subplots_adjust(top=((total_height - header + 1) / total_height))
        gs = gridspec.GridSpec(len(to_compare.keys()), 1, hspace=0.2)
        for scene,exps in to_compare.items():

            gss = gridspec.GridSpecFromSubplotSpec(1 + len(exps) - 1, 1, gs[scene.num-1, :], hspace=0.2)
            #ax = plt.subplot(gs[scene.num-1, :])
            ax = plt.subplot(gss[0])

            for exp in exps:
                x, y = zip(*read_tcpprobe(path.join(exp.loc, "probe.out")))
                x = np.array(x) - min(x)
                y = np.array(y)
                if args.downsample:
                    x = x[::args.downsample]
                    y = y[::args.downsample]
                plt.plot(x, y, color=colors[exp.key], label=exp.key, linestyle=exp.lt, alpha=0.7)
            handles, labels = ax.get_legend_handles_labels()
            by_label = OrderedDict(zip(labels, handles))
            plt.legend(by_label.values(), by_label.keys(), loc='upper right')
            ax.set_xlabel("Time (s)")
            ax.set_ylabel("CWND (pkts)")
            ax.set_title(str(scene), fontsize=16, fontweight='bold')
            ax.grid()


            gsss = gridspec.GridSpecFromSubplotSpec(len(exps),1,gss[1:], hspace=0)
            exps_top = header + ((scene.num - 1) * inches_per_scene) + (inches_per_scene / 2)
            exps_bottom = exps_top + (inches_per_scene / 2)
            exps_length = (exps_bottom - exps_top)
            for i, exp in enumerate(exps):
                ax = plt.subplot(gsss[i])

                mm_results = parse_mm_log(path.join(exp.loc, "uplink.log"),
                                            int(scene.rtt / 2))
                plt.plot(mm_results.time_vals, mm_results.tpt_vals, linewidth=2,
                         color=colors[exp.key], linestyle='-', alpha=0.85)

                #plt.figtext(0.05, in_from_top(exps_top + ( ((i) / 4.0) * exps_length) ), exp.key,
                #        fontsize=16,
                #        fontweight='bold',
                #        rotation='vertical'
                #)
                ax.text(0.02, 0.91, exp.key,
                        transform=ax.transAxes,
                        bbox=dict(facecolor='white', edgecolor='black', fill=True, alpha=1.0),
                        fontsize=13,
                        color=colors[exp.key],
                        fontweight='bold')
                ax.grid()
                rounded_max = int((ax.get_ylim()[1] * 1.2) / 2) * 2
                ax.set_ylim(0, rounded_max)
                ax2 = ax.twinx()
                ax2.plot(mm_results.time_vals, mm_results.del_vals, color='C5',
                linewidth=2, linestyle='-', alpha=0.85)
                rounded_max = int((ax2.get_ylim()[1] * 1.2) / 2) * 2
                ax2.set_ylim(0, rounded_max)
                if i == 0: 
                    ax.set_title("Mahimahi Uplink Log")
                ax.set_ylabel("Throughput (Mbps)", color=colors[exp.key])
                ax2.set_ylabel("Queue Delay (ms)", color='C5')
                if i == len(exps)-1:
                    ax.set_xlabel("Time (s)")

        plt.savefig('results.pdf')

        print "\nSaved to ./results.pdf"

