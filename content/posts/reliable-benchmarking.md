---
layout: post
title:  "Reliably Benchmarking Small Changes"
description: "Modern OSs and hardware make the task of benchmarking small performance gains difficult"
date:   2024-11-20
---

I often have to benchmark web services to see if some small change has a meaningful impact on performance. Typically, you spawn a few web service workers and use another program (ideally on another machine in the same network) to hammer that service. During this time, the test program will keep track of how many requests were processed and the latency for each of them. If throughput goes up and/or latency goes down, your change was effective. All of this seems straightforward, so what's the catch?

### Dynamic Nature of Hardware and OS

Modern hardware and operating systems are [complex and dynamic in nature](/p/neumann_architecture). This means two runs of the same benchmark will not necessarily produce the same result, even if you don't change anything. There are several factors affecting this, namely:

- Simultaneous Multi-Threading (SMT, CPU simultaneously executing 2 or more threads using 1 core)
- Boosted clock frequencies (Temporary boost on few cores aka "Turbo Boost")
- Thermal throttling
- Scheduling noise, other running processes

We need to control most of these factors to reduce measurement variance.


### Precision Over Accuracy

As this noise comes from features that opportunistically improve performance, removing this noise usually results in lower absolute performance. So your benchmarked values will be _inaccurate_ with respect to what you will typically observe on the same hardware. This is fine.

Benchmarks need to be repeatable i.e. precision matters more than accuracy. Why? When making changes to improve performance I am trying to compare a small change and its impact. An accurate but imprecise benchmark will have a variance that is bigger than the impact of a small change that I just made. E.g. If the expected improvement in performance from my change is 1% then it can not use a benchmark that has a variance of +/- 5%.

### Disable SMT

Simultaneous Multi-Threading (SMT) or "Hyper-Threading" is a feature in modern CPUs that allows one core to present itself as two or more hardware execution contexts or "threads". This way processor can work on 2 software threads at the same time. Bare minimum components inside the core are duplicated to support multiple threads and other components are shared. Most importantly, caches are shared between both threads.

This feature provides huge gains when one of the processes scheduled is memory intensive and another isn't ([ref](https://dl.acm.org/doi/10.1145/1133572.1133597)). This way while the core is waiting for memory access from one thread it can service another thread's compute needs. In practice, such an ideal mix of processes is rare, so you never see 2x benefit from SMT. Typical gains range from 1.1x to 1.3x speedup.

This behaviour however induces a lot of variance in benchmarks. We don't have control over what gets co-scheduled on the same core. So it's best to just disable this feature.

You can find how hardware threads are laid out in your CPU topology and then disable all but 1 thread from each core. You can check sibling threads using this command:

```
sudo cat /sys/devices/system/cpu/cpu*/topology/thread_siblings_list
```

In my case, I have 8 cores with 2 threads each, so I disable one thread from each core.

```bash
disabled_cpus=(1 3 5 7 9 11 13 15)
for cpu_no in $disabled_cpus
do
  echo 0 | sudo tee /sys/devices/system/cpu/cpu$cpu_no/online
done
```

Note: All commands shown in this post are risky, so do not attempt this in production instances. Shown commands are also specific to my machine, you'll have to do your own research and figure out the correct commands for your machine.

Tip: On AWS and other cloud providers, their "vCPU" is a hardware thread and not a core. So going from 1 vCPU to 2vCPU is more like 1.1-1.3x improvement and not 2x as you'd think.
There are some exceptions to this like the latest generation AMD instances (\*7a). It's best to verify the topology before benchmarking. You should also consider this fact while comparing instance types.


### Disable dynamic clock boost

CPUs have a certain power budget. In theory, if a CPU has P power budget and N cores, then each core has a power budget of roughly P/N. However, with some fancy tricks like "Turbo Boost", CPUs can boost the clock of a few active cores when other cores are idling to improve the single-threaded performance of whatever is executing on that core.

Several factors affect this behavior.

1. This boosted clock can not be sustained for long periods because it will cause a concentration of heat produced in boosted cores.
2. Due to manufacturing defects not all cores are made equal, only a few cores in your system might be able to boost to advertised boost frequencies. In the case of [AMD pstate driver](https://docs.kernel.org/admin-guide/pm/amd-pstate.html), the driver lets the kernel know which cores can boost.
3. Your CPU might be already hot, so it won't attempt to boost the clock.
4. Your other cores might not be idle, so your benchmarked core won't boost.

This behavior is a problem for repeatable results and you need to limit the clock to your "nominal" or "base" frequency. This frequency is guaranteed to be sustained for longer periods.

There are multiple ways to disable this. I don't have a BIOS option and I use the latest kernel with the pstate driver which doesn't have an option to fully disable this. I can however simulate the same behaviour by setting my nominal frequency (2.7GHz) to maximum frequency. A better option is available by setting AMD pstate driver in passive or guided mode and taking control of frequency. That requires fiddling with kernel boot parameters and I'd rather not fiddle with them every time I want to benchmark something.


```bash
enabled_cpus=(0 2 4 6 8 10 12 14)
for cpu_no in $enabled_cpus
do
  echo 2700000 | sudo tee /sys/devices/system/cpu/cpufreq/policy$cpu_no/scaling_max_freq
done
```

### Pick Better Scaling Governor

If you are using a laptop to benchmark things, most likely default CPU scaling governor will be optimized for saving power. You want to use the performance governor instead to tell OS you don't care about saving power (for now).


```bash
enabled_cpus=(0 2 4 6 8 10 12 14)
for cpu_no in $enabled_cpus
do
  echo performance | sudo tee /sys/devices/system/cpu/cpu$cpu_no/cpufreq/scaling_governor
done
```

[This Arch Wiki page](https://wiki.archlinux.org/title/CPU_frequency_scaling) has in-depth information about various drivers and governors you'll find in the wild.


### Remove Scheduler Noise

Ideally, you should strive to eliminate unnecessary processes when running benchmarks. If you have more runnable processes than available cores then it's quite likely that the program you're benchmarking will not get the entire core for itself. Frequent context switches and CPU migrations can also induce slowdowns from colder caches.

You can avoid this by pinning your process to a single CPU using `taskset`, if you want even better surety you can reserve certain cores specifically for your benchmarked programs using [cset](https://documentation.suse.com/sle-rt/12-SP5/html/SLE-RT-all/cha-shielding-cpuset.html).


### Disable ASLR

This is a security feature of Linux that randomizes address space so multiple instances of the same program don't have the same layout in memory. This makes it harder to perform certain memory-based attacks. This however, also adds variability in your results ([ref](https://users.cs.northwestern.edu/~robby/courses/322-2013-spring/mytkowicz-wrong-data.pdf)). While in practice I didn't find this variation to be large for high-level web services, I disable it anyway because it doesn't take much time.

```bash
echo 0 | sudo tee /proc/sys/kernel/randomize_va_space
```

### Warmup and Cooldown

If what you're benchmarking will run repeatedly or for long periods of time, then warming up by running benchmark (but not measuring) will ensure all relevant caches are warmed up. This includes CPU caches and page caches. Conversely, if you don't expect warm caches in production then you should drop caches before each benchmark run.

Cooldown sounds weird, but on laptop devices thermal throttling is a problem. Regardless of configuration changes, if your CPU gets too hot it will start throttling, this is mostly outside of your control. So between two benchmarking runs, let the processor temperatures reach reasonable numbers.

You should also keep your laptop plugged in to avoid performance throttling that is done to preserve power.

### Results

I benchmarked a very dumb Python web service that just responds to a ping with a JSON response and I was able to reduce the noise by 6x. This kind of repeatability isn't possible with more complex examples, but similar noise reduction is still seen. Following is throughput (req/s) numbers of 5 runs before and after quiescing, last row contains range of values in form of (max - min) / average.


| Before | After  |
| :-:    | :-:    |
| 653.43 | 387.67 |
| 638.39 | 388.82 |
| 639.42 | 389.11 |
| 650.53 | 388.01 |
| 652.12 | 388.05 |
|        |        |
| 2.33%  | 0.37%  |




### Mechanical Sympathy

I only touched upon one part of reliable benchmarking. While doing this plus statistical analysis will go a long way, it's still not enough to reliably conclude very small changes in performance.

You need to develop some ["Mechanical Sympathy"](https://mechanical-sympathy.blogspot.com/2011/07/why-mechanical-sympathy.html) with your code and hardware to know if that change is valid. E.g. If I am eliminating one unnecessary copy, I don't need to see 1% of performance improvement for me to consider this change. It should be done just because that copy is wasteful. Ignoring these "micro-optimizations" is how you get the death of performance by 1000 cuts.


\- - -

Fun Fact: The Microsoft engineer that detected the XZ backdoor was trying to [quiesce the system](https://www.openwall.com/lists/oss-security/2024/03/29/4) for benchmarking and noticed odd results from SSHD. Armed with this knowledge, now you too could be the next person who discovers a backdoor or writes a backdoor that isn't susceptible to detection by perf-obsessed engineers.


### References/Further Reading

- This post is an expanded version of [LLVM benchmarking tips](https://llvm.org/docs/Benchmarking.html)
- [Simultaneous multithreading](https://en.wikipedia.org/wiki/Simultaneous_multithreading)
- [CPU Performance Scaling on Linux](https://docs.kernel.org/admin-guide/pm/cpufreq.html)
- [Kernel address space layout randomization](https://lwn.net/Articles/569635/)
- [Mechanical Sympathy](https://mechanical-sympathy.blogspot.com/2011/07/why-mechanical-sympathy.html)

