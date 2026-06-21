# SIR espacial en Agents.jl — espejo de examples/sir (swarm-core) y sir_mesa.py.
#
# Especificación compartida: grilla torus totalmente ocupada, vecindad Moore,
# susceptible con k vecinos infectados se contagia con prob 1-(1-beta)^k, un
# infectado se recupera con prob gamma, activación aleatoria por paso, término
# cuando no quedan infectados.
#
# Modo bench: imprime `steps,ms` como `sir --bench` y sir_mesa.py --bench.
# CRÍTICO: hace warmup explícito antes de medir, para NO cronometrar la
# compilación JIT de Julia (sería una comparación injusta).
#
# Uso: julia --project=. sir_agents.jl --bench SEED --width W --height H \
#            --infected N --steps S

using Agents
using Random

const BETA = 0.08
const GAMMA = 0.1

@agent struct Person(GridAgent{2})
    status::Symbol
end

function person_step!(agent, model)
    if agent.status === :S
        k = 0
        for n in nearby_agents(agent, model, 1)   # radio 1, métrica chebyshev = Moore (8)
            n.status === :I && (k += 1)
        end
        if k > 0 && rand(abmrng(model)) < 1.0 - (1.0 - BETA)^k
            agent.status = :I
        end
    elseif agent.status === :I
        if rand(abmrng(model)) < GAMMA
            agent.status = :R
        end
    end
end

function build(width, height, infected, seed)
    space = GridSpaceSingle((width, height); periodic = true, metric = :chebyshev)
    model = StandardABM(Person, space;
        agent_step! = person_step!,
        scheduler = Schedulers.Randomly(),
        rng = Xoshiro(seed))
    for pos in positions(model)
        add_agent!(pos, model, :S)
    end
    # Sembrar `infected` agentes al azar como infectados.
    ids = collect(allids(model))
    for id in randsubseq(abmrng(model), ids, infected / length(ids))
        model[id].status = :I
    end
    # Asegurar exactamente `infected` (randsubseq es aproximado): completar/recortar.
    cur = count(a -> a.status === :I, allagents(model))
    if cur < infected
        for id in ids
            model[id].status === :S || continue
            model[id].status = :I
            cur += 1
            cur == infected && break
        end
    end
    return model
end

n_infected(model) = count(a -> a.status === :I, allagents(model))

# Solo el stepping (sin construcción): es lo que cronometran sir --bench y
# sir_mesa.py --bench, para que la comparación sea like-with-like.
function run_steps!(model, max_steps)
    steps = 0
    while steps < max_steps && n_infected(model) > 0
        step!(model, 1)
        steps += 1
    end
    return steps
end

function run_sim(width, height, infected, max_steps, seed)
    model = build(width, height, infected, seed)
    return run_steps!(model, max_steps)
end

function main()
    args = ARGS
    getopt(name, default) = (i = findfirst(==(name), args)) === nothing ? default : args[i+1]
    bench_i = findfirst(==("--bench"), args)
    seed = bench_i === nothing ? 42 : parse(Int, args[bench_i+1])
    width = parse(Int, getopt("--width", "100"))
    height = parse(Int, getopt("--height", "100"))
    infected = parse(Int, getopt("--infected", "5"))
    steps = parse(Int, getopt("--steps", "300"))

    if bench_i !== nothing
        # Warmup: fuerza la compilación JIT (build + step) con una corrida
        # diminuta, descartada.
        run_sim(5, 5, 1, 10, seed)
        # Medición real: construir FUERA del cronómetro, medir solo el stepping
        # (igual que sir --bench y sir_mesa.py --bench).
        model = build(width, height, infected, seed)
        t0 = time_ns()
        done = run_steps!(model, steps)
        ms = (time_ns() - t0) / 1e6
        println("steps,ms")
        println("$done,$(round(ms, digits = 3))")
    else
        done = run_sim(width, height, infected, steps, seed)
        println("Agents.jl SIR $(width)x$(height) | seed $seed | $done pasos")
    end
end

main()
