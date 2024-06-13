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

graph_data = []
for file in os.listdir(directory):
    filename = os.fsdecode(file)
    runtime, threads = filename.split('-')
    if runtime == "SPAR_RUST_MPI":
        runtime = "Our Abstraction"
    elif runtime == "MPI":
        runtime = "Raw MPI"
    

    values = read_floats(filename)
    m = mean(values)
    dev = stddev(values, m)

    graph_data.append((runtime, int(threads), m, dev))

graph_data.sort()

plot_data = {}
plot_data["Our Abstraction"] = ([], [], [])
plot_data["Raw MPI"] = ([], [], [])

for (runtime, thread, mean, stddev) in graph_data:
    plot_data[runtime][0].append(thread)
    plot_data[runtime][1].append(mean)
    plot_data[runtime][2].append(stddev)

plt.errorbar(
    plot_data["Our Abstraction"][0],
    plot_data["Our Abstraction"][1],
    plot_data["Our Abstraction"][2],
    label = "Our Abtraction"
)

plt.errorbar(
    plot_data["Raw MPI"][0],
    plot_data["Raw MPI"][1],
    plot_data["Raw MPI"][2],
    label = "Raw MPI"
)

plt.legend()
plt.savefig(graph_name + ".svg")
