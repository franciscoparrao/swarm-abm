# Figura: rendimiento cross-engine (ms/paso) de swarm-abm vs Agents.jl vs Mesa,
# en SIR y Schelling. Escala log: muestra los órdenes de magnitud de separación.
# Correr desde la raíz del repo: Rscript paper/R/fig_performance.R

source("paper/R/theme_paper.R")
suppressPackageStartupMessages({ library(dplyr); library(tidyr); library(readr); library(patchwork) })

read_engine <- function(path, model) {
  read_csv(path, show_col_types = FALSE) |>
    transmute(model = model, agents,
              `swarm-abm` = rust_mspp, `Agents.jl` = agentsjl_mspp, Mesa = mesa_mspp) |>
    pivot_longer(c(`swarm-abm`, `Agents.jl`, Mesa),
                 names_to = "engine", values_to = "mspp")
}

sir <- read_engine("validation/data/cross_engine_summary.csv", "a) SIR")
sch <- read_engine("validation/data/cross_engine_schelling_summary.csv", "b) Schelling")
dat <- bind_rows(sir, sch) |>
  mutate(engine = factor(engine, levels = c("swarm-abm", "Agents.jl", "Mesa")))

# Una etiqueta de RANGO de speedup por panel, en la zona vacía inferior-derecha.
ann <- dat |>
  pivot_wider(names_from = engine, values_from = mspp) |>
  mutate(vs_mesa = Mesa / `swarm-abm`, vs_ajl = `Agents.jl` / `swarm-abm`) |>
  group_by(model) |>
  summarise(lab = sprintf("vs Mesa: %.0f–%.0f×\nvs Agents.jl: %.1f–%.1f×",
                          min(vs_mesa), max(vs_mesa), min(vs_ajl), max(vs_ajl)),
            .groups = "drop") |>
  mutate(agents = 40000, mspp = 0.018)

p <- ggplot(dat, aes(agents, mspp, colour = engine, shape = engine)) +
  geom_line(linewidth = 0.6) +
  geom_point(size = 1.8) +
  geom_text(data = ann, aes(x = agents, y = mspp, label = lab),
            inherit.aes = FALSE, hjust = 1, vjust = 0,
            size = 2.5, lineheight = 0.95, colour = "grey25") +
  facet_wrap(~ model, nrow = 1) +
  scale_x_log10(breaks = c(625, 2500, 10000, 40000),
                labels = c("625", "2.5k", "10k", "40k")) +
  scale_y_log10(breaks = c(0.01, 0.1, 1, 10, 100),
                labels = c("0.01", "0.1", "1", "10", "100")) +
  scale_colour_manual(values = engine_colors) +
  scale_shape_manual(values = c("swarm-abm" = 16, "Agents.jl" = 17, "Mesa" = 15)) +
  labs(x = "Agents (grid cells)", y = "Time per step (ms)") +
  theme_paper() +
  theme(panel.grid.major.x = element_line(linewidth = 0.2, colour = "grey90"))

save_paper(p, "paper/figs/fig_performance.pdf", width = 18, height = 7.5)
