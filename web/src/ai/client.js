import { BOARD_SIZE } from '../core/constants.js';
import { level } from './difficulty.js';
export function createAiClient({
  createWorker,
  canAcceptResponse = () => true,
  onProgress = () => {},
  onMove = () => {},
  onError = () => {},
  onThinkingChange = () => {},
  reportError = console.error,
  schedule = setTimeout,
  cancelSchedule = clearTimeout,
  now = () => performance.now(),
  progressTickMs = 120,
}) {
  let worker = null,
    nextRequestId = 0,
    pendingRequestId = null,
    resultTimer = null,
    progressTimer = null;
  let bestMove = null,
    completedDepth = 0,
    position = null,
    restartAfterForce = false,
    progressFloor = 0,
    startedAt = 0,
    budgetMs = 0,
    maxDepth = 1,
    milestone = 0,
    milestoneAt = 0,
    iterationMs = 0;

  // The engine reports once per completed iteration, and those arrive further and further apart as it
  // deepens, so a bar driven only by them jumps and then stands still. Two estimates fill the gaps, both
  // monotone and neither able to claim a depth the engine has not reached:
  //
  //   - inside the interval the *next* iteration owns (the bar approaches it without overtaking it), paced
  //     by how long the last iterations took - they grow geometrically, hence the factor;
  //   - along the level's wall clock, which is what the engine is actually bounded by.
  function nextMilestone() {
    return Math.min(95, Math.round((100 * (completedDepth + 1)) / maxDepth));
  }
  function tickProgress() {
    progressTimer = null;
    if (pendingRequestId === null || budgetMs <= 0) return;
    const elapsed = now() - startedAt;
    const pace = Math.max(150, iterationMs * 3);
    const inside =
      milestone +
      Math.max(0, nextMilestone() - milestone) * (1 - Math.exp(-(now() - milestoneAt) / pace));
    reportProgress(Math.min(95, Math.round(Math.max((100 * elapsed) / budgetMs, inside))));
    if (pendingRequestId !== null)
      progressTimer = schedule(tickProgress, progressTickMs);
  }
  function startProgress(difficulty) {
    stopProgress();
    const level_ = level(difficulty);
    progressFloor = 0;
    budgetMs = level_.timeMs;
    maxDepth = level_.maxDepth;
    milestone = 0;
    iterationMs = 0;
    startedAt = now();
    milestoneAt = startedAt;
    progressTimer = schedule(tickProgress, progressTickMs);
  }
  function stopProgress() {
    cancelSchedule(progressTimer);
    progressTimer = null;
  }
  function reportProgress(progress) {
    progressFloor = Math.max(progressFloor, progress);
    onProgress(progressFloor);
  }
  // A completed iteration sets the floor and starts the next interval's timer.
  function reportMilestone(progress) {
    milestone = Math.max(milestone, progress);
    const at = now();
    iterationMs = Math.max(0, at - milestoneAt);
    milestoneAt = at;
    reportProgress(milestone);
  }
  function cancel() {
    pendingRequestId = null;
    bestMove = null;
    completedDepth = 0;
    restartAfterForce = false;
    onThinkingChange(false);
    stopProgress();
    cancelSchedule(resultTimer);
    resultTimer = null;
    const retired = worker;
    worker = null;
    retired?.terminate();
  }
  function fail(source, error) {
    if (source !== worker) return;
    const hadRequest = pendingRequestId !== null;
    cancel();
    reportError('AI Worker error:', error);
    if (hadRequest && canAcceptResponse()) onError(error);
  }
  function initialize() {
    cancel();
    try {
      const source = createWorker();
      worker = source;
      source.onmessage = ({ data }) => {
        const { type, move, progress, requestId, depth } = data || {};
        if (
          source !== worker ||
          pendingRequestId === null ||
          requestId !== pendingRequestId ||
          !canAcceptResponse()
        )
          return;
        if (type === 'SEARCH_ITERATION') {
          if (
            Number.isInteger(depth) &&
            depth > completedDepth &&
            legal(move)
          ) {
            bestMove = move;
            completedDepth = depth;
          }
        } else if (type === 'PROGRESS_UPDATE') reportMilestone(progress);
        else if (type === 'BEST_MOVE_FOUND' && resultTimer === null) {
          if (legal(move)) bestMove = move;
          reportProgress(100);
          stopProgress();
          resultTimer = schedule(() => {
            if (
              source !== worker ||
              requestId !== pendingRequestId ||
              !canAcceptResponse()
            )
              return;
            resultTimer = null;
            pendingRequestId = null;
            onThinkingChange(false);
            onMove(move);
          }, 150);
        }
      };
      source.onerror = (error) => fail(source, error);
      source.onmessageerror = (error) => fail(source, error);
      return true;
    } catch (error) {
      reportError('Failed to create AI worker:', error);
      worker = null;
      return false;
    }
  }
  function findMove(data, fallbackMove = null) {
    if (pendingRequestId !== null || !canAcceptResponse()) return;
    if (!worker && restartAfterForce) initialize();
    if (!worker) {
      onError();
      return;
    }
    position = data.position || data;
    completedDepth = 0;
    bestMove = legal(fallbackMove) ? fallbackMove : null;
    pendingRequestId = ++nextRequestId;
    onThinkingChange(true);
    startProgress(data.difficulty);
    const source = worker;
    try {
      source.postMessage({
        type: 'FIND_BEST_MOVE',
        requestId: pendingRequestId,
        data,
      });
    } catch (error) {
      fail(source, error);
    }
  }
  function legal(move) {
    if (
      !position ||
      !move ||
      !Number.isInteger(move.row) ||
      !Number.isInteger(move.col) ||
      move.row < 0 ||
      move.row >= BOARD_SIZE ||
      move.col < 0 ||
      move.col >= BOARD_SIZE
    )
      return false;
    const index = move.row * BOARD_SIZE + move.col;
    return (
      ((position.blackBitboard[index >>> 5] |
        position.whiteBitboard[index >>> 5]) &
        (1 << (index % 32))) ===
      0
    );
  }
  function forceMove() {
    if (pendingRequestId === null || !canAcceptResponse() || !bestMove)
      return false;
    const move = bestMove;
    cancel(); // terminate synchronous search; a worker cannot process a stop message while searching
    restartAfterForce = true;
    onMove(move);
    return true;
  }
  return {
    initialize,
    forceMove,
    get completedDepth() {
      return completedDepth;
    },
    findMove,
    cancel,
    dispose: cancel,
    get available() {
      return worker !== null;
    },
    get pendingRequestId() {
      return pendingRequestId;
    },
  };
}
