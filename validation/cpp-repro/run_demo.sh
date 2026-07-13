#!/usr/bin/env bash
set -u
service postgresql start >/dev/null 2>&1
for i in $(seq 1 20); do pg_isready -q && break; sleep 0.5; done
su postgres -c "psql -q -c \"ALTER USER postgres PASSWORD 'secret';\""
su postgres -c "psql -q -tc \"SELECT 1 FROM pg_database WHERE datname='sim-db'\" | grep -q 1 \
  || psql -q -c 'CREATE DATABASE \"sim-db\";'" >/dev/null 2>&1
su postgres -c "psql -q -d sim-db -f /app/database/create_sim_db.sql" >/dev/null 2>&1
PSQL() { su postgres -c "psql -d sim-db -A -t -c \"$1\""; }

cd /app
CFG=sim_config.json
mkdir -p out

for RUN in 1 2; do
  ./bin/agro-sim -c "$CFG" > /tmp/r$RUN.out 2> /tmp/r$RUN.err
  dur=$(grep -oE 'SIM DURATION: [0-9]+' /tmp/r$RUN.out | grep -oE '[0-9]+')
  echo "CORRIDA $RUN — exit $? — SIM DURATION reportada: ${dur} ms — config: $CFG (idéntica)"
done

echo
echo "== Filas escritas por cada ejecución (aggregated_product_results) =="
PSQL "SELECT execution_id, count(*) AS filas FROM aggregated_product_results GROUP BY execution_id ORDER BY execution_id;"

echo
echo "== Hash MD5 del vector completo de resultados, por ejecución =="
PSQL "
WITH ex AS (SELECT execution_id FROM execution ORDER BY execution_id DESC LIMIT 2)
SELECT r.execution_id,
       md5(string_agg(r.process||'|'||coalesce(r.variable_type,'')||'|'||r.time||'|'||r.product_id||'|'||r.value::text,
            ',' ORDER BY r.process,r.variable_type,r.time,r.product_id)) AS hash_resultados
FROM aggregated_product_results r JOIN ex USING (execution_id)
GROUP BY r.execution_id ORDER BY r.execution_id;"

echo
echo "== Cuántas filas (mismas claves) tienen valor DISTINTO entre las 2 corridas =="
PSQL "
WITH ex AS (SELECT execution_id FROM execution ORDER BY execution_id DESC LIMIT 2),
ids AS (SELECT min(execution_id) a, max(execution_id) b FROM ex)
SELECT count(*) FILTER (WHERE a.value <> b.value) AS filas_distintas,
       count(*)                                   AS filas_comparadas
FROM aggregated_product_results a
JOIN ids ON a.execution_id=ids.a
JOIN aggregated_product_results b
  ON b.execution_id=ids.b AND b.process=a.process
     AND b.variable_type IS NOT DISTINCT FROM a.variable_type
     AND b.time=a.time AND b.product_id=a.product_id;"

echo
echo "== Ejemplo de filas que difieren (misma clave, distinto valor) =="
PSQL "
WITH ex AS (SELECT execution_id FROM execution ORDER BY execution_id DESC LIMIT 2),
ids AS (SELECT min(execution_id) a, max(execution_id) b FROM ex)
SELECT a.process, a.time, a.product_id,
       round(a.value::numeric,3) AS corrida_A, round(b.value::numeric,3) AS corrida_B
FROM aggregated_product_results a
JOIN ids ON a.execution_id=ids.a
JOIN aggregated_product_results b
  ON b.execution_id=ids.b AND b.process=a.process
     AND b.variable_type IS NOT DISTINCT FROM a.variable_type
     AND b.time=a.time AND b.product_id=a.product_id
WHERE a.value <> b.value
ORDER BY abs(a.value-b.value) DESC LIMIT 8;"

echo
echo "== VEREDICTO =="
PSQL "
WITH ex AS (SELECT execution_id FROM execution ORDER BY execution_id DESC LIMIT 2),
h AS (SELECT r.execution_id,
        md5(string_agg(r.process||'|'||coalesce(r.variable_type,'')||'|'||r.time||'|'||r.product_id||'|'||r.value::text,
             ',' ORDER BY r.process,r.variable_type,r.time,r.product_id)) hh
      FROM aggregated_product_results r JOIN ex USING (execution_id) GROUP BY r.execution_id)
SELECT CASE
  WHEN (SELECT count(*) FROM h) < 2 THEN 'INCONCLUSO: menos de 2 ejecuciones con resultados'
  WHEN count(DISTINCT hh)=1 THEN 'REPRODUCIBLE: las 2 corridas son identicas'
  ELSE 'NO REPRODUCIBLE: 2 corridas con config identica dan resultados distintos'
END FROM h;"
