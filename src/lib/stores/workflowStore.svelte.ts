/**
 * Workflow Store — built-in and custom workflow templates.
 * Workflows are text instructions injected into CLAUDE.local.md
 * to guide agent behavior within a session.
 */

export interface WorkflowTemplate {
  id: string;
  name: string;
  instructions: string;
  builtin: boolean;
}

const BUILTIN_WORKFLOWS: WorkflowTemplate[] = [
  {
    id: 'dev',
    name: 'Development',
    builtin: true,
    instructions: `Каждую задачу выполняй по пайплайну:
1. PM intake — изучи задачу, контекст, связанные файлы, acceptance criteria
2. Architect plan — спроектируй подход, определи файлы, зависимости
3. Implementation — напиши код
4. Final review — проведи ревью от всех агентов:
   - Architect: архитектура, паттерны, качество кода
   - Security: OWASP, секреты, инъекции, доступы
   - Tester: покрытие тестами критического кода
   - PM: соответствие требованиям, scope
5. Iterate — если ревью нашло проблемы, пофикси и проведи ревью заново
6. Commit — conventional commit

Не спрашивай подтверждений между фазами.
Не пропускай ревью. Итерируй ревью пока все агенты не одобрят.`,
  },
  {
    id: 'refactor',
    name: 'Refactoring',
    builtin: true,
    instructions: `Рефакторинг по пайплайну:
1. Architect analysis — code smells, метрики, scope
2. Characterization tests — захвати текущее поведение тестами (GREEN)
3. Refactor — маленькие шаги, тесты после каждого
4. Final review — architect (метрики before/after) + security (нет регрессий)
5. Iterate until approved
6. Commit each logical step separately

НЕ меняй поведение. Только структуру.`,
  },
  {
    id: 'review-only',
    name: 'Review Only',
    builtin: true,
    instructions: `Проведи code review:
- Architect: архитектура, паттерны
- Security: уязвимости
- Tester: покрытие
Не пиши код, только анализ и рекомендации.`,
  },
];

class WorkflowStore {
  workflows = $state<WorkflowTemplate[]>([...BUILTIN_WORKFLOWS]);

  getById(id: string): WorkflowTemplate | undefined {
    return this.workflows.find((w) => w.id === id);
  }

  getInstructions(id: string | undefined): string | null {
    if (!id) return null;
    const wf = this.getById(id);
    return wf?.instructions || null;
  }

  /** All workflows for dropdown options (including "None"). */
  get options(): { value: string; label: string }[] {
    return [
      { value: '', label: 'None' },
      ...this.workflows.map((w) => ({ value: w.id, label: w.name })),
    ];
  }
}

export const workflowStore = new WorkflowStore();
