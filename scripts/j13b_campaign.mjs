import { ARMS, CELL_MATRIX } from './j13b_schema.mjs';

export { CELL_MATRIX };

function randomFor(seed) {
  let state = [...seed].reduce((value, char) => Math.imul(value ^ char.charCodeAt(0), 16777619) >>> 0, 2166136261);
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state;
  };
}

export function childKey(job) {
  return `${job.cell_id}|${job.arm}|${job.block}|${job.repeat}`;
}

export function campaignSchedule(seed = 'j13b-fixed-prereg-v1') {
  const random = randomFor(seed);
  const jobs = [];
  for (let block = 0; block < 12; block++) {
    const offset = block % ARMS.length;
    const armOrder = [...ARMS.slice(offset), ...ARMS.slice(0, offset)];
    for (let repeat = 0; repeat < 2; repeat++) {
      const cells = [...CELL_MATRIX];
      for (let index = cells.length - 1; index > 0; index--) {
        const other = random() % (index + 1);
        [cells[index], cells[other]] = [cells[other], cells[index]];
      }
      const rotation = block % cells.length;
      const ordered = [...cells.slice(rotation), ...cells.slice(0, rotation)];
      for (const cell of ordered) {
        for (const arm of armOrder) {
          if (cell.arms.includes(arm)) {
            jobs.push({ cell_id: cell.id, scenario: cell.scenario, arm, block, repeat });
          }
        }
      }
    }
  }
  return jobs;
}

export function plannedRunCount() {
  return CELL_MATRIX.reduce((sum, cell) => sum + cell.arms.length * 12 * 2, 0);
}
