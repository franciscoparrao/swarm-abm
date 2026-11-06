// SIGRID en C++ — Hito 1: subconjunto de screening (ovejas + zorros culpeo,
// sin perros/liebres/chillas). Port fiel del modelo swarm-abm committeado
// (models/sigrid/src/lib.rs @ HEAD), que es el oráculo de validación.
//
// Determinismo: sembrado (mt19937_64). El RNG difiere del ChaCha8 de swarm-abm,
// así que la validación es DISTRIBUCIONAL (Pearson/Spearman sobre puntos de
// parámetros × semillas), no bit-exacta — misma metodología que la paridad vs
// Mesa (models/sigrid/PARITY.md).
//
// Semántica replicada del motor: el índice espacial es una INSTANTÁNEA tomada al
// inicio del paso (before_step); las consultas ven posiciones y estado de inicio
// de tick, mientras las mutaciones (mover, matar, miedo) ocurren sobre el arreglo
// vivo. Activación aleatoria (orden barajado por paso con el RNG maestro).
//
// Uso: sheep_fox [--days N] [--seed N] [--seeds N] [--sheep-density D]
//                [--fox-density D] [--fox-eff E] [--lamb-prop P]
#include "sigrid_core.hpp"

static double run_loss_rate(const Params& params_in, uint64_t seed, uint64_t n_days, long& killed, long& attempts) {
    // Construcción de la población (fuente única en sigrid_core.hpp): agentes,
    // rasters, y las listas de fase A (ovejas/liebres) / fase B (zorros/perros).
    Params params = params_in;   // copia modificable: el raster real fija width/height
    Model m; std::vector<int> phaseA, phaseB, foxidx;
    if (load_farm_raster(m, params.farm_raster_path)) {
        params.width  = m.farm_cols * m.farm_csz;
        params.height = m.farm_rows * m.farm_csz;
    }
    // Potreros (cercos internos): si se pidieron y no cargan, ABORTAR (nunca correr
    // sin cercos en silencio). Sin --paddocks es un no-op -> bit-idéntico.
    if (params.paddock_raster_path && !load_paddocks(m, params.paddock_raster_path, params.units_path)) {
        std::fprintf(stderr, "abortando: no se pudo cargar el raster de potreros\n"); std::exit(2);
    }
    // Estación: --season y --season-sim van juntos; cualquiera sin el otro, o un error
    // de carga, ABORTA. Sin ambos es un no-op -> bit-idéntico.
    if ((params.season_path != nullptr) != (params.season_sim >= 0)) {
        std::fprintf(stderr, "abortando: --season y --season-sim van juntos\n"); std::exit(2);
    }
    if (params.season_path && !load_season(m, params.season_path)) {
        std::fprintf(stderr, "abortando: no se pudo cargar la estacion\n"); std::exit(2);
    }
    if (params.dog_roam && !params.paddock_raster_path) {
        std::fprintf(stderr, "abortando: --dog-roam requiere --paddocks\n"); std::exit(2);
    }
    build_model(params, seed, m, phaseA, phaseB, foxidx);
    load_cascos(m, params.cascos_path);   // casas reales (ancla de cacería); no-op si null
    // Vegetación real: sobrescribe los rásters sintéticos ya generados (y ya usados para
    // nada todavía: la población no depende de ellos). Si se pidió y no carga, ABORTAR.
    if (params.veg_quality_path && !load_veg_raster(m.veg_quality, params.veg_quality_path, "calidad")) {
        std::fprintf(stderr, "abortando: no se pudo cargar la calidad de forraje\n"); std::exit(2); }
    if (params.veg_cover_path && !load_veg_raster(m.veg_cover, params.veg_cover_path, "cobertura")) {
        std::fprintf(stderr, "abortando: no se pudo cargar la cobertura\n"); std::exit(2); }
    // Contadores por estancia (solo acumulan; no alteran la simulación).
    { const size_t nf = (size_t)std::max(1, m.n_farms);
      m.sheep_init_farm.assign(nf, 0); m.sheep_killed_farm.assign(nf, 0); m.outings_farm.assign(nf, 0);
      m.culpeos_hunted_farm.assign(nf, 0); m.chillas_hunted_farm.assign(nf, 0);
      m.culpeo_hours_farm.assign(nf, 0.0); m.chilla_hours_farm.assign(nf, 0.0);
      for (auto& a : m.agents) if (a.alive && a.species == SHEEP) m.sheep_init_farm[a.farm]++; }
    // Estancias que cazan (--hunt-farms "1,0,..."): mismo número de valores que estancias.
    if (params.hunt_farms) {
        const char* s = params.hunt_farms; m.hunt_farm_mask.clear();
        while (*s) { char* e; long v = std::strtol(s, &e, 10);
            if (e == s || (v != 0 && v != 1)) { std::fprintf(stderr, "abortando: --hunt-farms invalido: \"%s\"\n", params.hunt_farms); std::exit(2); }
            m.hunt_farm_mask.push_back((char)v); s = (*e == ',') ? e + 1 : e;
            if (*e && *e != ',') { std::fprintf(stderr, "abortando: --hunt-farms invalido: \"%s\"\n", params.hunt_farms); std::exit(2); } }
        if ((int)m.hunt_farm_mask.size() != m.n_farms) { std::fprintf(stderr, "abortando: --hunt-farms da %zu estancias; el modelo tiene %d\n", m.hunt_farm_mask.size(), m.n_farms); std::exit(2); }
    }
    const bool farm_out = params.farm_out_path != nullptr;
    // Buffers reusados para la detección de zorros precomputada (Hito 4b).
    std::vector<std::vector<int>> fdprey(m.agents.size());
    std::vector<int> fdhares(m.agents.size(), 0);
    for (uint64_t step = 0; step < n_days * 24; ++step) {
        m.before_step();
        // Fase A (paralela): RNG por-agente (seed,step,id) -> independiente del orden de hilos.
        #pragma omp parallel for schedule(static)
        for (size_t a = 0; a < phaseA.size(); ++a) {
            int idx = phaseA[a]; if (!m.agents[idx].alive) continue;
            PRng r(agent_seed(seed, step, (uint64_t)idx));
            if (m.agents[idx].species == SHEEP) step_sheep(m, idx, r); else step_hare(m, idx, r);
        }
        // Fase B1 (paralela): detección de presas de cada zorro (read-only sobre el
        // snapshot; los zorros están en su posición de inicio de tick).
        #pragma omp parallel for schedule(dynamic, 16)
        for (size_t k = 0; k < foxidx.size(); ++k) {
            int idx = foxidx[k];
            if (!m.agents[idx].alive) { fdprey[idx].clear(); continue; }
            fox_detect(m, m.agents[idx].x, m.agents[idx].y, fdprey[idx], fdhares[idx]);
        }
        // Fase B2 (secuencial): orden barajado por tick (determinista); zorros y
        // perros mutan estado compartido (selección/ataque/disuasión).
        { PRng shuf(agent_seed(seed, step, 0xFFFFFFFFFFFFFFFFULL));
          for (size_t k = phaseB.size(); k > 1; --k) { size_t j = (size_t)(shuf.unit() * k); std::swap(phaseB[k - 1], phaseB[j]); } }
        for (int idx : phaseB) { if (!m.agents[idx].alive) continue;
            PRng r(agent_seed(seed, step, (uint64_t)idx));
            if (m.agents[idx].species == FOX) step_fox(m, idx, r, fdprey[idx], fdhares[idx]); else step_dog(m, idx); }
        // Fase C (secuencial): nacimientos de liebres decididos en la fase A y
        // mortalidad denso-dependiente una vez al día, como en model_v2.step().
        if (hare_births(m, phaseA) > 0) { fdprey.resize(m.agents.size()); fdhares.resize(m.agents.size(), 0); }
        if (step % 24 == 0) hare_cull(m);
        hunt_events(m, step, seed);   // cacería humana (inerte si hunt_prob<=0)
        // presencia de zorros por estancia (horas-zorro): mide el desplazamiento de H1
        if (farm_out) for (auto& a : m.agents) if (a.alive && a.species == FOX) {
            const int f = m.farm_of(a.x, a.y); (a.is_chilla ? m.chilla_hours_farm : m.culpeo_hours_farm)[f] += 1.0; }
        // término anticipado si no quedan ovejas
        bool any_sheep = false; for (auto& a : m.agents) if (a.alive && a.species == SHEEP) { any_sheep = true; break; }
        if (!any_sheep) break;
    }
    if (farm_out) {
        FILE* fo = std::fopen(params.farm_out_path, "a");
        if (!fo) { std::fprintf(stderr, "abortando: no pude abrir %s\n", params.farm_out_path); std::exit(2); }
        std::fseek(fo, 0, SEEK_END);
        if (std::ftell(fo) == 0) std::fprintf(fo, "seed,farm,hunts,sheep_init,sheep_killed,loss_pct,culpeo_hours,chilla_hours,outings,culpeos_hunted,chillas_hunted\n");
        for (int f = 0; f < std::max(1, m.n_farms); ++f) {
            const int hunts = m.params.hunt_mode == 1 ? (m.hunt_farm_mask.empty() ? 1 : (int)m.hunt_farm_mask[f]) : (m.params.hunt_prob > 0 ? 1 : 0);
            std::fprintf(fo, "%llu,%d,%d,%ld,%ld,%.4f,%.0f,%.0f,%ld,%ld,%ld\n", (unsigned long long)seed, f, hunts,
                         m.sheep_init_farm[f], m.sheep_killed_farm[f],
                         m.sheep_init_farm[f] > 0 ? 100.0 * m.sheep_killed_farm[f] / m.sheep_init_farm[f] : 0.0,
                         m.culpeo_hours_farm[f], m.chilla_hours_farm[f], m.outings_farm[f], m.culpeos_hunted_farm[f], m.chillas_hunted_farm[f]);
        }
        std::fclose(fo);   // se escribe al terminar cada semilla: persistencia incremental
    }
    if (getenv("SIGRID_DIAG")) {
        long nh = 0, ns = 0; double hare_e = 0;
        for (auto& a : m.agents) { if (!a.alive) continue;
            if (a.species == HARE) { nh++; hare_e += a.energy; }
            if (a.species == SHEEP) ns++; }
        const double area_ha = (m.params.width * m.params.height) / 10000.0;
        fprintf(stderr, "  [dog] persecuciones %ld | rastro %ld | arreo %ld\n", m.n_chase, m.n_trail, m.n_herd);
        { long nl = 0, ns_ = 0; for (auto& a : m.agents) if (a.alive && a.species == HARE) { nl++; if (m.veg_quality.get(a.x, a.y) <= 0.0) ns_++; }
          fprintf(stderr, "  [liebres] vivas %ld (%.3f/ha del dominio) | en celdas de calidad 0 (mar) %ld | cazadas por zorros %ld | energía media %.1f\n",
                  nl, nl / std::max(area_ha, 1.0), ns_, m.hares_killed, nh > 0 ? hare_e / nh : 0.0); }
        // cercos: violaciones (agente cercado fuera de su granja / potrero) y ovejas por unidad
        long viol_farm = 0, viol_unit = 0; std::vector<long> per_unit(std::max(1, m.n_units), 0);
        for (auto& a : m.agents) { if (!a.alive || (a.species != SHEEP && a.species != DOG)) continue;
            if (m.n_farms > 1 && m.farm_of(a.x, a.y) != a.farm) viol_farm++;
            // el perro con --dog-roam cruza potreros a propósito: no es violación
            if (m.has_paddocks()) { if (m.unit_of(a.x, a.y) != a.unit && !(a.species == DOG && m.params.dog_roam)) viol_unit++; if (a.species == SHEEP) per_unit[a.unit]++; } }
        fprintf(stderr, "  [cercos] granjas %d | potreros %d | violaciones granja %ld | violaciones potrero %ld | zorros cazados %ld\n",
                m.n_farms, m.n_units, viol_farm, viol_unit, m.foxes_hunted);
        if (m.has_paddocks()) { fprintf(stderr, "  [ovejas/potrero]"); for (int u = 0; u < m.n_units; ++u) fprintf(stderr, " u%d(f%d)=%ld", u, m.unit_farm[u], per_unit[u]); fprintf(stderr, "\n"); }
        { long nc = 0, nch = 0;
          for (auto& a : m.agents) if (a.alive && a.species == FOX) (a.is_chilla ? nch : nc)++;
          fprintf(stderr, "  [caza] salidas %ld | culpeos cazados %ld | chillas cazadas %ld | vivos al final: culpeos %ld chillas %ld\n",
                  m.hunt_outings, m.culpeos_hunted, m.chillas_hunted, nc, nch);
          fprintf(stderr, "  [perro] distancia max. al centro de su rebaño: %.0f m (correa %.0f m)\n", m.dog_max_flock_dist, m.params.dog_leash); }
        if (!m.unit_season.empty()) { long s0 = 0, s1 = 0; for (int u = 0; u < m.n_units; ++u) (m.unit_season[u] == 0 ? s0 : s1) += per_unit[u];
            fprintf(stderr, "  [estacion] simulada %d | ovejas en invernada %ld | en veranada %ld\n", m.params.season_sim, s0, s1); }
        { std::vector<long> dpf(m.n_farms, 0); std::vector<long> dpu(std::max(1, m.n_units), 0);
          for (auto& a : m.agents) if (a.alive && a.species == DOG) { dpf[a.farm]++; if (m.has_paddocks()) dpu[m.unit_of(a.x, a.y)]++; }
          fprintf(stderr, "  [perros/estancia]"); for (int f = 0; f < m.n_farms; ++f) fprintf(stderr, " f%d=%ld", f, dpf[f]);
          if (m.has_paddocks()) { fprintf(stderr, " | en potrero:"); for (int u = 0; u < m.n_units; ++u) if (dpu[u]) fprintf(stderr, " u%d=%ld", u, dpu[u]); }
          fprintf(stderr, "\n"); }
    }
    killed = m.sheep_killed; attempts = m.predation_attempts;
    return (double)m.sheep_killed / std::max(1, m.n_sheep_initial) * 100.0;
}

int main(int argc, char** argv) {
    Params p; uint64_t days = 30, seed0 = 1000, n_seeds = 1;
    auto darg = [&](const char* n, double dv) -> double { for (int i = 1; i < argc - 1; ++i) if (!std::strcmp(argv[i], n)) return std::atof(argv[i + 1]); return dv; };
    auto sarg = [&](const char* n) -> const char* { for (int i = 1; i < argc - 1; ++i) if (!std::strcmp(argv[i], n)) return argv[i + 1]; return nullptr; };
    p.farm_raster_path = sarg("--farm-raster");   // raster real de granjas (fija el dominio)
    p.cascos_path = sarg("--cascos");             // casas reales por granja (ancla de cacería)
    p.paddock_raster_path = sarg("--paddocks");   // raster de potreros (cercos internos; requiere --farm-raster)
    p.units_path = sarg("--units");               // tabla de potreros: area real + punto interior (reparto del rebaño)
    p.season_path = sarg("--season");             // estación por unidad (requiere --paddocks)
    p.season_sim = (int)darg("--season-sim", -1); // 0 invernada, 1 veranada (requiere --season)
    if (p.season_sim > 1) { std::fprintf(stderr, "abortando: --season-sim debe ser 0 o 1\n"); return 2; }
    p.dog_roam = darg("--dog-roam", 0) != 0;      // perro cruza cercos internos y sigue al grupo mayor
    p.dogs_farm = sarg("--dogs-farm");            // perros por estancia "n0,n1,..." (reemplaza --dogs)
    p.dog_leash = darg("--dog-leash", 0.0);       // distancia máxima del perro al centro de su rebaño (m)
    p.hunt_mode = (int)darg("--hunt-mode", 0);    // 1 = cacería régimen B
    p.hunt_p_culpeo = darg("--hunt-p-culpeo", 0.0);
    p.hunt_chilla_ratio = darg("--hunt-chilla-ratio", p.hunt_chilla_ratio);
    p.hunt_radius = darg("--hunt-radius", p.hunt_radius);
    p.hunt_outing_prob = darg("--hunt-outing-prob", p.hunt_outing_prob);   // régimen A: prob. baja de salir
    p.hunt_farms = sarg("--hunt-farms");          // estancias que cazan "1,0,0,0,1"
    p.farm_out_path = sarg("--farm-out");         // CSV por estancia y semilla
    p.veg_quality_path = sarg("--veg-quality");   // calidad de forraje real (formato .farms)
    p.veg_cover_path = sarg("--veg-cover");       // cobertura real (formato .farms)
    p.cover_effect = darg("--cover-effect", p.cover_effect);   // coef. cobertura en la captura (default +0,05)
    days = (uint64_t)darg("--days", 30); seed0 = (uint64_t)darg("--seed", 1000); n_seeds = (uint64_t)darg("--seeds", 1);
    p.sheep_density = darg("--sheep-density", p.sheep_density);
    p.fox_density = darg("--fox-density", p.fox_density);
    p.fox_predation_effectiveness = darg("--fox-eff", p.fox_predation_effectiveness);
    p.lamb_proportion = darg("--lamb-prop", p.lamb_proportion);
    p.chilla_density = darg("--chilla-density", p.chilla_density);
    p.hare_density = darg("--hare-density", p.hare_density);
    p.n_dogs = (int)darg("--dogs", p.n_dogs);
    p.farms_x = (int)darg("--farms-x", p.farms_x); p.farms_y = (int)darg("--farms-y", p.farms_y);
    p.hunt_prob = darg("--hunt-prob", p.hunt_prob);   // cacería: prob. por granja/día
    set_switch_lambda(darg("--switch-lambda", 1.0));  // fuerza del prey switching
    p.width = darg("--width", p.width);   // escalar el AREA a densidad constante
    p.height = darg("--height", p.height); // -> muchos agentes, pocos vecinos/query
    // validaciones de cacería y correa: DESPUÉS de leer todos los flags (--hunt-prob incluido)
    if (p.hunt_mode != 0 && p.hunt_mode != 1) { std::fprintf(stderr, "abortando: --hunt-mode debe ser 0 o 1\n"); return 2; }
    if (p.hunt_mode == 1 && p.hunt_prob > 0) { std::fprintf(stderr, "abortando: --hunt-prob es del régimen 0; con --hunt-mode 1 usar --hunt-p-culpeo\n"); return 2; }
    if (p.hunt_mode == 1 && !(p.hunt_p_culpeo > 0 && p.hunt_p_culpeo <= 1)) { std::fprintf(stderr, "abortando: --hunt-mode 1 requiere 0 < --hunt-p-culpeo <= 1\n"); return 2; }
    if (!(p.hunt_outing_prob > 0 && p.hunt_outing_prob <= 1)) { std::fprintf(stderr, "abortando: --hunt-outing-prob debe estar en (0, 1]\n"); return 2; }
    if ((p.hunt_farms || p.hunt_outing_prob < 1) && p.hunt_mode != 1) { std::fprintf(stderr, "abortando: --hunt-farms y --hunt-outing-prob requieren --hunt-mode 1\n"); return 2; }
    if (p.hunt_chilla_ratio < 0 || p.hunt_p_culpeo * p.hunt_chilla_ratio > 1 || p.hunt_radius <= 0 || p.dog_leash < 0) { std::fprintf(stderr, "abortando: parámetros de cacería o correa fuera de rango\n"); return 2; }

    double sum = 0; int cnt = 0;
    for (uint64_t s = 0; s < n_seeds; ++s) {
        long killed = 0, attempts = 0;
        double lr = run_loss_rate(p, seed0 + s, days, killed, attempts);
        printf("  semilla %llu: loss_rate %.2f%% | matadas %ld | intentos %ld\n",
               (unsigned long long)(seed0 + s), lr, killed, attempts);
        sum += lr; cnt++;
    }
    printf("\nloss_rate medio %.2f%% sobre %llu semillas\n", sum / std::max(1, cnt), (unsigned long long)n_seeds);
    return 0;
}
