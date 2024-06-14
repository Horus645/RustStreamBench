import sys
import math
import os
import matplotlib.pyplot as plt

def read_floats(filename):
    file_content = open(sys.argv[1] + "/" + filename, "r")
    floats = []
    for line in file_content.readlines():
        floats.append(float(line))
    return floats

def mean(values):
    if len(values) == 0: return 0

    m = 0
    for value in values: m += value
    return m / len(values)

def stddev(values, mean):
    if len(values) == 0: return 0

    dev = 0
    for value in values: dev += math.pow(value - mean, 2)
    return math.sqrt(dev / len(values))

if len(sys.argv) < 2:
    print("usage: " + sys.argv[0] + " <directory in [data]> <graph name>")
    exit(-1)


graph_name = sys.argv[2]

directory = os.fsencode(sys.argv[1])

sequential_mean = 0
sequential_stddev = 0
graph_data = []
for file in os.listdir(directory):
    filename = os.fsdecode(file)

    values = read_floats(filename)
    m = mean(values)
    dev = stddev(values, m)

    try:
        runtime, threads = filename.split('-')
        if runtime == "SPAR_RUST_MPI":
            runtime = "Our Abstraction"
        elif runtime == "MPI":
            runtime = "Raw MPI"
        graph_data.append((runtime, int(threads), m, dev))
    except ValueError:
        if filename == "SEQUENTIAL":
            sequential_mean = m
            sequential_stddev = dev
    

graph_data.sort()

plot_data = {}
plot_data["Our Abstraction"] = ([1], [1], [sequential_stddev], [100])
plot_data["Raw MPI"] = ([1], [1], [sequential_stddev], [100])
plot_data["Ideal"] = ([1], [1], [sequential_stddev])

for (runtime, thread, mean, stddev) in graph_data:
    plot_data[runtime][0].append(thread)
    plot_data[runtime][1].append(sequential_mean / mean)
    plot_data[runtime][2].append(stddev)
    plot_data[runtime][3].append(100 * ((sequential_mean / mean) / thread))

    if plot_data["Ideal"][0][-1] < thread:
        plot_data["Ideal"][0].append(thread)
        plot_data["Ideal"][1].append(thread)
        plot_data["Ideal"][2].append(0)


fig, ax1 = plt.subplots()
ax2 = ax1.twinx()

ax1.set_zorder(ax2.get_zorder()+1) # put ax in front of ax2
ax1.patch.set_visible(False) # hide the 'canvas'
ax2.patch.set_visible(True) # show the 'canvas'

ax2.bar(
    [x - 0.3 for x in plot_data["Raw MPI"][0] if x != 1],
    plot_data["Raw MPI"][3][1:],
    0.6,
    label = "Raw MPI",
    fill = False,
    color = "crimson",
    hatch = "\\"
)

ax2.bar(
    [x + 0.3 for x in plot_data["Our Abstraction"][0] if x != 1],
    plot_data["Our Abstraction"][3][1:],
    0.6,
    label = "Our Abtraction",
    fill = False,
    color = "aqua",
    hatch = "-"
)

ax1.errorbar(
    plot_data["Ideal"][0],
    plot_data["Ideal"][1],
    plot_data["Ideal"][2],
    color="blue",
    label = "Ideal",
    fmt = '.',
    elinewidth = 0.01,
    markersize = 5,
    linestyle = "dotted",
)

ax1.errorbar(
    plot_data["Raw MPI"][0],
    plot_data["Raw MPI"][1],
    plot_data["Raw MPI"][2],
    color="orange",
    label = "Raw MPI",
    fmt = 's',
    linestyle = "dashed",
    markersize = 2.5,
    elinewidth = 0.01,
    capsize = 3,
)

ax1.errorbar(
    plot_data["Our Abstraction"][0],
    plot_data["Our Abstraction"][1],
    plot_data["Our Abstraction"][2],
    color="green",
    label = "Our Abtraction",
    fmt = '^',
    linestyle = "dashdot",
    markersize = 2.5,
    elinewidth = 0.01,
    capsize = 3,
)

ax1.set_xlabel("Workers")
ax1.set_ylabel("Speedup")
ax1.legend(loc=0, bbox_to_anchor=(0.335,1.22))

ax2.set_ylabel("Efficiency")
ax2.legend(loc=0, bbox_to_anchor=(1,1.1645))

ax1.set_xticks(plot_data["Ideal"][0])
ax1.set_yticks([1, 5, 10, 15, 20, 25, 30, 35, 40, 45])

plt.savefig(graph_name + ".svg", bbox_inches='tight')
