// static/dev_engine.js — Developer rules
const DevEngine = {
    detectUseCase(prompt) {
        const p = prompt.toLowerCase();
        if (/\b(bug|error|exception|stack trace|crash)\b/.test(p))  return 'code';
        if (/\b(meeting|transcript|action item|minutes)\b/.test(p)) return 'transcript';
        if (/\b(contract|clause|liability|legal)\b/.test(p))        return 'legal';
        if (/\b(resume|candidate|job|hiring)\b/.test(p))            return 'resume';
        if (/\b(revenue|earnings|financial|quarter)\b/.test(p))     return 'financial';
        if (/\b(paper|study|research|findings)\b/.test(p))          return 'research';
        if (/\b(ticket|support|customer|complaint)\b/.test(p))      return 'ticket';
        return 'generic';
    },

    // task-specific shorthand templates (applied before sending to backend)
    applyTemplates(text) {
        return text
            .replace(/i'?m using react and typescript/gi, "Tech: React, TS")
            .replace(/next\.?js with tailwind/gi, "Tech: Next.js, Tailwind")
            .replace(/postgresql database/gi, "DB: PostgreSQL")
            .replace(/i want to/gi, "Goal:")
            .replace(/the expected output is/gi, "Output:");
    }
};
