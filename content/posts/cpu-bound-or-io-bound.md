---
layout: post
title:  "Is Your Web Service Really I/O Bound?"
description: "Not knowing the true nature of your program can result in months of futile engineering work."
date:   2024-11-22
discussions:
    HackerNews: "https://news.ycombinator.com/item?id=42215384"
    LinkedIn: "https://www.linkedin.com/posts/ankushmenat_is-your-web-service-really-io-bound-activity-7265751393125216257-51ey"
---


> I read that most web services that talk to DB are I/O Bound. <br>
> I am working on a web service that talks to DB. <br>
> Therefore, I have an I/O bound service. <br>
> Since I have I/O bound service, I should rewrite it in event-driven architecture to improve CPU utilization.
>
> -- A hypothetical dialogue in my head.


Web services are [often cited](https://stackoverflow.com/questions/868568/what-do-the-terms-cpu-bound-and-i-o-bound-mean) as example of typical I/O bound service and this is _typically_ correct, but this may not be true for your specific service. It is essential to properly study your service and workload it services in production, failing to do so can result in engineering efforts that yield sub-optimal results.

"Systems Performance" by Brendan Gregg calls this method of studying workload "Workload Characterization".

### CPU Bound Vs. I/O Bound

You might have encountered this way of classifying processes from OS literature, where different processes have different scheduling needs. I/O bound processes use disk or networking to communicate with other processes like database or cache. This means they often yield CPU long before their timeslice expires, on the other hand, CPU Bound processes tend to use their entire timeslice.

Schedulers improve CPU utilization by running other processes while I/O bound processes are waiting for a response from disk or network. This same logic applies to the performance of individual processes too. If the I/O bound process frequently yields CPU then it can not saturate available CPU with useful work. Two popular approaches to improving CPU utilization are:

1. Event-driven architecture (async-await, "event loops")
2. User-space schedulers (goroutines, greenlet)

Both models have the same goal: Avoid yielding CPU while performing blocking I/O and do some other useful work instead. E.g. start servicing another request or do some other computation that doesn't depend on this I/O. Keep in mind that what we are trying to save here is the cost of frequent context switches, which are typically 1-20 microseconds on Linux depending on what you consider as cost. Both models still have some overhead, albeit far lower than switching threads.


### Measuring I/O bound-edness

How do you go about measuring this property of processes? If you use tools like `iostat` they'll show IO wait times. But this is probably not what you want. Web services tend to be idle a lot and wake up when a new request arrives. So if your service doesn't have any load right now or is under-loaded then it will be "waiting for I/O" most of the time. That does make it I/O bound as per textbook definition, but this way of classification is also entirely useless for us.

We need to hammer our service with the maximum possible workload it can sustain and measure how much time is spent on the CPU and how much time is spent on waiting for I/O. To do this, I'll suggest starting with certain assumptions to simplify things:

1. Your process uses the good-old synchronous model of handling one request from start to end and then moving on to the next request. If it's already using event driven model or userspace scheduler then measuring this is significantly harder. You'll likely have to instrument your runtime to get these statistics out.
2. Use a single process that is also single-threaded.
3. Prepare a test client that will generate a "representative workload" or at least something close enough.

Let's start with most simplest, dumbest measurement tool: the inbuilt `time` utility. Start the process and immediately start the test client, after that wait for a sufficient amount of time to elapse and kill the service.

```bash
λ time gunicorn 'app:app'
^C
12.67s user 1.45s system 91% cpu 15.482 total
```

You have probably used `time` before to measure how much time a process takes but might not be familiar with other numbers.

1. "User" time is time spent in userspace - your application, and the libraries that it is using are accounted here.
2. "System" time is time spent in the kernel on behalf of your application - this is the time kernel spent servicing your system calls.
3. "Total" time is wall clock time. This is user + system + wait time. This is also called "Real" time in some shells.

Now you can get the wait time percentage just by doing `1 - ( user + system ) / real`. In this case, it turns out to be `1 - (12.67 + 1.45) / 15.482 = 9%`. If you check the output of `time` again, the complement of wait time i.e. time on CPU is already computed for you: `91% cpu`.

Not so fast. This number is total wait time, not just "I/O wait" time. This wait time includes:

- Disk I/O wait - This includes system calls like `read` and `fsync`.
- Networking I/O wait - This includes `recv` and `accept`/`epoll_wait`, this is why you need a workload that can saturate the process.
- Explicit sleep calls from your app like `time.sleep(1)`
- Lock wait - Futex or any other synchronization primitives your app might be using.
- Scheduling delays - if there are more runnable process than available CPU threads, your process can be delayed off-CPU.

So while this number is inaccurate, it still establishes a strict upper bound of waiting that our application is doing. This might be enough to conclude whether the process is I/O bound or CPU bound during peak workload.

One key takeaway: Workloads define whether the process is I/O bound or not and it's dynamic.

In our case, it's quite clear that this workload is CPU-bound. But for the sake of correctness, let's use a more accurate and granular measurement of where our process is wasting time by waiting.

### Accurate and Granular Measures

On-CPU time can be understood by using sampling profilers like `perf` but the time spent off-CPU needs different tools. This is where [BPF Compiler Collection](https://github.com/iovisor/bcc) (bcc) comes in handy.

`bcc` comes with eBPF based off-CPU time profiler called `offcputime`. You can use it to see which system calls spent how much time waiting off the CPU. To use it, first, you need to start your service and workload generator and then ask offcputime to profile the process for 5 seconds.

```bash
offcputime -K -p `pgrep gunicorn` 5
```

`-K` means we only care about the kernel stack. You can drop it to get the entire callstack, that will significantly increase the size of recorded data and probably perturb the process. Usually, I don't care about the source of issued I/O.

Running offcputime time gives us output like this:

```bash
Tracing off-CPU time (us) of PIDs 39421 by kernel stack for 5 secs.

... ( random small values clipped) ...

    finish_task_switch.isra.0
    schedule
    irqentry_exit_to_user_mode
    irqentry_exit
    sysvec_reschedule_ipi
    asm_sysvec_reschedule_ipi
    -                gunicorn: worke (39421)
        454

    finish_task_switch.isra.0
    schedule
    schedule_timeout
    wait_woken
    sk_wait_data
    tcp_recvmsg_locked
    tcp_recvmsg
    inet_recvmsg
    sock_recvmsg
    __sys_recvfrom
    __x64_sys_recvfrom
    x64_sys_call
    do_syscall_64
    entry_SYSCALL_64_after_hwframe
    -                gunicorn: worke (39421)
        403347
```

Roughly 0.41 seconds of 5 seconds were spent in waiting for I/O. This means I/O wait time is `0.41 / 5 = 8.2%`, so `time` was pretty close!

I recommend checking out [example document](https://github.com/iovisor/bcc/blob/master/tools/offcputime_example.txt) for how to use this tool along with this awesome article from Brendan Gregg on [Off-CPU analysis techniques](https://www.brendangregg.com/offcpuanalysis.html). If you drop `-K` flag then you can also create a flamegraph to see which part of your application is causing the longest waits.

In this case, after eliminating other wait types I can conclude that I/O wait times are somewhere in the range of 5%-10% which means this workload and this code in its current state is definitely not I/O bound and hence it won't see significant benefits from a rewrite to event-driven model.


### Why my service isn't I/O bound?

I don't have a concrete answer but here are a few hypotheses:

1. There could be a lot of CPU bound processing for each request. Not all web services are simple CRUD.
2. My generated workload just doesn't do enough I/O. Realistic production traces can be used to invalidate this.
3. There could be waste present in the current code and eliminating that will eventually make it I/O bound. In this case, Linux's context-switch cost is not something where I should spend time on.
4. The service under inspection is written in Python. So CPU bound work is as it is at least 2 orders of magnitude slower than what it would be in some compiled language.
5. The services that it's interacting with are tried and tested databases written in C. Maybe they are fast enough.

It could be anything. The point of this post is not to answer it for this specific case but to learn to ask the right questions and _characterize_ the workload before optimizing it.

---


### References / Further Reading

- [Systems Performance (book)](https://www.brendangregg.com/blog/2020-07-15/systems-performance-2nd-edition.html)
- [Performance Analysis Methodology](https://www.brendangregg.com/methodology.html)
- If you don't know how web servers work, you won't have _mechanical sympathy_ for what is going on. So, [build your own web server](https://github.com/codecrafters-io/build-your-own-x?tab=readme-ov-file#build-your-own-web-server).
- [Memory Bound Workload](https://en.wikipedia.org/wiki/Memory-bound_function)
- [Reliably Benchmarking Small Changes](/p/reliable-benchmarking)
