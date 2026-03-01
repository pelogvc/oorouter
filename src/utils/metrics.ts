export function createDurationMetrics(startTime: number) {
  const totalNs = (Date.now() - startTime) * 1_000_000
  return {
    total_duration: totalNs,
    load_duration: 0,
    prompt_eval_count: 0,
    prompt_eval_duration: 0,
    eval_count: 0,
    eval_duration: totalNs,
  }
}
