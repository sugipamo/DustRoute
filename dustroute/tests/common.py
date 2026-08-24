from dustroute import *


def settle(world,n=8):
    sim=RedstoneTickSimulator(world)
    state=sim.snapshot()
    for _ in range(n):
        state=sim.step()
    return state
