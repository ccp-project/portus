import sys, subprocess

if len(sys.argv) != 3:
    sys.exit("error: test-inner.py expected 2 arguments: [1]: length, [2]: output_dir")

try:
    exp_length = int(sys.argv[1])
except:
    sys.exit("error: could not parse {} as integer experiment length".format(sys.argv[1]))
output_dir = sys.argv[2]

subprocess.Popen("iperf -c $MAHIMAHI_BASE -p 4242 -i 1 -Z ccp -t {length}"
                 " > {out}/send.out 2>&1".format(
    length=exp_length,
    out=output_dir
), shell=True).wait()
