import unittest
from random import randint, choice
from string import lowercase
import threading
import time

f = open("/dev/ccpkp", "r+", 10)

class TestCorrectness(unittest.TestCase):
    def test_single_write(self):
        s = "testing a single write"
        f.write(s)
        f.flush()
        self.assertEqual(s,f.read(len(s)))

    def test_sequential_writes(self):
        ss = ["a","bc","def","ghij","klmno","pqrstuvwxyz"]
        for s in ss:
            f.write(s)
            f.flush()
        for s in ss:
            got = f.read(len(s))
            self.assertEqual(s,got)

    def test_rand_rw(self):
        for i in range(10):
            nwrites = randint(1,5)
            bytes_written = 0
            full_s = ""
            for i in range(nwrites):
                s = "".join(choice(lowercase) for _ in range(randint(5,50)))
                f.write(s)
                f.flush()
                bytes_written += len(s)
                full_s += s
            got = f.read(bytes_written)
            self.assertEqual(full_s, got)

    def test_wrap(self):
        long_s = "x" * 3500
        f.write(long_s)
        f.flush()
        got = f.read(len(long_s))
        self.assertEqual(long_s, got)

"""
Not working yet.


class TestMulti(unittest.TestCase):
    def test_two_writers(self):
        def writer(num):
            for i in range(21):
                #print num,i
                s = str(num) * 10
                f.write(s)
                #f.flush()

            could_be = [str(num)*10 for num in range(1,10)]
            for i in range(20):
                got = f.read(10)
                #print num,got
                self.assertIn(got, could_be)
    
        workers = []
        for num in range(5):
            workers.append(threading.Thread(target=writer, args=(num+1,)))
        for worker in workers:
            worker.start()
        for worker in workers:
            worker.join()
"""

if __name__ == "__main__":
    unittest.main()
