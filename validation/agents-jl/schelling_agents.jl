# Schelling en Agents.jl — espejo de examples/schelling (swarm-core) y
# schelling_mesa.py.
#
# Especificación compartida: grilla torus, densidad 0.85, dos grupos 50/50,
# vecindad Moore, conforme si similitud >= 0.375 (1.0 si aislado), el inconforme
# se muda a una celda vacía uniforme al azar, activación aleatoria por paso.
#
# Modo bench: pasos fijos (sin corte por convergencia), cronometra solo el
# stepping, con warmup JIT previo. Imprime steps,ms.
#
# Uso: julia --project=. schelling_agents.jl --bench SEED --width W --height H --steps S

using Agents
using Random

const DENSITY = 0.85
const TOLERANCE = 0.375

@agent struct Resident(GridAgent{2})
    group::Int
end

function schelling_step!(agent, model)
    same = 0
    total = 0
    for n in nearby_agents(agent, model, 1)   # Moore (chebyshev, radio 1)
        total += 1
        n.group == agent.group && (same += 1)
    end
    sim = total == 0 ? 1.0 : same / total
    if sim < TOLERANCE
        move_agent_single!(agent, model)       # a una celda vacía al azar
    end
end

function build(width, height, seed)
    space = GridSpaceSingle((width, height); periodic = true, metric = :chebyshev)
    model = StandardABM(Resident, space;
        agent_step! = schelling_step!,
        scheduler = Schedulers.Randomly(),
        rng = Xoshiro(seed))
    pos_shuf = shuffle(abmrng(model), collect(positions(model)))
    n_agents = round(Int, width * height * DENSITY)
    for (i, pos) in enumerate(pos_shuf[1:n_agents])
        add_agent!(pos, model, (i - 1) % 2)
    end
    return model
end

# Solo stepping, pasos fijos (sin corte por convergencia).
function run_steps!(model, steps)
    for _ in 1:steps
        step!(model, 1)
    end
end

function main()
    args = ARGS
    getopt(name, default) = (i = findfirst(==(name), args)) === nothing ? default : args[i+1]
    bench_i = findfirst(==("--bench"), args)
    seed = bench_i === nothing ? 42 : parse(Int, args[bench_i+1])
    width = parse(Int, getopt("--width", "50"))
    height = parse(Int, getopt("--height", "50"))
    steps = parse(Int, getopt("--steps", "100"))

    if bench_i !== nothing
        # Warmup: compila build + step con una corrida diminuta (descartada).
        run_steps!(build(8, 8, seed), 5)
        # Medición: construir fuera del cronómetro, medir solo el stepping.
        model = build(width, height, seed)
        t0 = time_ns()
        run_steps!(model, steps)
        ms = (time_ns() - t0) / 1e6
        println("steps,ms")
        println("$steps,$(round(ms, digits = 3))")
    else
        model = build(width, height, seed)
        run_steps!(model, steps)
        println("Agents.jl Schelling $(width)x$(height) | seed $seed | $steps pasos")
    end
end

main()
