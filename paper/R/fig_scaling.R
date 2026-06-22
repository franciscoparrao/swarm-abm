# Figura: escalabilidad del decide paralelo (speedup vs hilos), separando
# decisiones compute-bound y memory-bound, con la referencia lineal ideal.
# Correr desde la raíz del repo: Rscript paper/R/fig_scaling.R

source("paper/R/theme_paper.R")
suppressPackageStartupMessages({ library(dplyr); library(readr) })

dat <- read_csv("validation/data/scaling_summary.csv", show_col_types = FALSE) |>
  mutate(regime = factor(ifelse(work == 50, "compute-bound", "memory-bound (Game of Life)"),
                         levels = c("compute-bound", "memory-bound (Game of Life)")))

ideal <- data.frame(threads = c(1, 16))

lab <- dat |> filter(threads == 16) |>
  mutate(txt = sprintf("%.1f×", speedup_vs_seq))

reg_cols <- c("compute-bound" = unname(wong["blue"]),
              "memory-bound (Game of Life)" = unname(wong["orange"]))

p <- ggplot(dat, aes(threads, speedup_vs_seq, colour = regime, shape = regime)) +
  geom_line(data = ideal, aes(threads, threads), inherit.aes = FALSE,
            linetype = "22", linewidth = 0.4, colour = "grey60") +
  annotate("text", x = 13.5, y = 15.2, label = "ideal", size = 2.4,
           colour = "grey55", angle = 38) +
  geom_line(linewidth = 0.6) +
  geom_point(size = 1.9) +
  geom_text(data = lab, aes(label = txt), hjust = -0.25, vjust = 0.4,
            size = 2.6, show.legend = FALSE) +
  scale_x_continuous(trans = "log2", breaks = c(1, 2, 4, 8, 16),
                     limits = c(1, 19)) +
  scale_y_continuous(breaks = c(1, 4, 8, 12, 16), limits = c(0, 16.5),
                     expand = expansion(mult = c(0.01, 0.03))) +
  scale_colour_manual(values = reg_cols) +
  scale_shape_manual(values = c(16, 17)) +
  guides(colour = guide_legend(nrow = 2), shape = guide_legend(nrow = 2)) +
  labs(x = "Threads", y = "Speedup vs sequential") +
  theme_paper()

save_paper(p, "paper/figs/fig_scaling.pdf", width = 9, height = 8)
