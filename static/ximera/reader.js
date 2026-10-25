// Interatividade das atividades do Ximera na leitura do Traduz: dicas,
// múltipla escolha, selecione todas, escolha de palavra, caixas de resposta
// dentro das fórmulas e comentários liberados após a tentativa. As respostas
// ficam no localStorage do navegador, por atividade.
(function () {
  'use strict';

  var activity = document.getElementById('theActivity');
  if (!activity) { return; }

  var TEXT = {
    check: 'Verificar',
    correct: 'Correto',
    tryAgain: 'Tente de novo',
    revealHint: 'Revelar dica',
    hint: 'Dica',
    chooseWord: 'escolha…',
    answerPlaceholder: 'resposta',
    freeResponse: 'Escreva sua resposta aqui.',
    resetConfirm: 'Apagar todas as respostas dadas nesta atividade?'
  };

  var storageKey = 'traduz:reader:' + (activity.getAttribute('data-path') || location.pathname);
  var progressKey = 'traduz:progress';
  var state = loadState();
  var answerSpecs = loadAnswerSpecs();
  var problems = [];

  function loadState() {
    try {
      return JSON.parse(localStorage.getItem(storageKey) || '{}') || {};
    } catch (error) {
      return {};
    }
  }

  function saveState() {
    try { localStorage.setItem(storageKey, JSON.stringify(state)); } catch (error) { /* modo privado */ }
  }

  function loadAnswerSpecs() {
    var specs = {};
    var scripts = activity.querySelectorAll('script.xm-answers');
    for (var i = 0; i < scripts.length; i++) {
      try {
        var parsed = JSON.parse(scripts[i].textContent || '{}');
        for (var key in parsed) { specs[key] = parsed[key]; }
      } catch (error) { /* ignora */ }
    }
    return specs;
  }

  function stateOf(id) {
    if (!state[id]) { state[id] = {}; }
    return state[id];
  }

  function element(tag, className, html) {
    var node = document.createElement(tag);
    if (className) { node.className = className; }
    if (html !== undefined) { node.innerHTML = html; }
    return node;
  }

  function closestProblem(node) {
    var parent = node.parentNode;
    while (parent && parent !== activity) {
      if (parent.classList && parent.classList.contains('problem-environment')) { return parent; }
      parent = parent.parentNode;
    }
    return activity;
  }

  function fire(node, name) {
    node.dispatchEvent(new CustomEvent('ximera:' + name, { bubbles: true }));
  }

  // --- Botões de verificação -------------------------------------------------

  function controls(onCheck) {
    var box = element('span', 'xm-controls');
    var submit = element('button', 'btn btn-primary btn-ximera-submit', '<i class="fa fa-question"></i>&nbsp;' + TEXT.check);
    var correct = element('button', 'btn btn-success btn-ximera-correct', '<i class="fa fa-check"></i>&nbsp;' + TEXT.correct);
    var incorrect = element('button', 'btn btn-danger btn-ximera-incorrect', '<i class="fa fa-times"></i>&nbsp;' + TEXT.tryAgain);
    [submit, correct, incorrect].forEach(function (button) {
      button.type = 'button';
      box.appendChild(button);
    });
    submit.addEventListener('click', onCheck);
    incorrect.addEventListener('click', onCheck);
    correct.hidden = true;
    incorrect.hidden = true;
    box.render = function (status, enabled) {
      submit.hidden = status !== 'pending';
      correct.hidden = status !== 'correct';
      incorrect.hidden = status !== 'incorrect';
      submit.disabled = !enabled;
    };
    return box;
  }

  // --- Múltipla escolha e selecione todas ----------------------------------

  function setupChoices(selector, multiple) {
    var containers = activity.querySelectorAll(selector);
    for (var i = 0; i < containers.length; i++) { setupChoice(containers[i], multiple); }
  }

  function setupChoice(container, multiple) {
    var id = container.id || ('choice-' + Math.random());
    var saved = stateOf(id);
    var choices = Array.prototype.filter.call(container.children, function (child) {
      return child.classList.contains('choice');
    });
    if (!choices.length) { return; }
    var group = element('div', 'btn-group-vertical');
    group.setAttribute('role', 'group');
    var buttons = choices.map(function (choice) {
      var button = element('button', 'text-left btn btn-secondary');
      button.type = 'button';
      button.dataset.choice = choice.id;
      button.dataset.correct = choice.classList.contains('correct') ? '1' : '';
      var input = document.createElement('input');
      input.type = multiple ? 'checkbox' : 'radio';
      input.tabIndex = -1;
      button.appendChild(input);
      choice.removeAttribute('id');
      button.appendChild(choice);
      group.appendChild(button);
      return button;
    });
    var box = controls(check);
    var wrapper = element('div', 'ximera-horizontal');
    wrapper.appendChild(group);
    wrapper.appendChild(box);
    container.appendChild(wrapper);

    function chosen() {
      if (multiple) { return saved.chosen || []; }
      return saved.chosen ? [saved.chosen] : [];
    }

    function render() {
      var picked = chosen();
      buttons.forEach(function (button) {
        var isChosen = picked.indexOf(button.dataset.choice) >= 0;
        button.classList.toggle('active', isChosen);
        button.firstChild.checked = isChosen;
        var wrong = !multiple && saved.wrong && saved.wrong[button.dataset.choice];
        var lockedOut = saved.correct && !button.dataset.correct;
        button.classList.toggle('disabled', Boolean(wrong || lockedOut));
      });
      var status = saved.correct ? 'correct'
        : (saved.checked && sameSet(saved.checked, picked) ? 'incorrect' : 'pending');
      box.render(status, picked.length > 0);
    }

    function check() {
      var picked = chosen();
      if (!picked.length) { return; }
      saved.checked = picked.slice();
      saved.attempts = (saved.attempts || 0) + 1;
      var correct = buttons.every(function (button) {
        return Boolean(button.dataset.correct) === (picked.indexOf(button.dataset.choice) >= 0);
      });
      saved.correct = correct;
      if (!correct && !multiple) {
        saved.wrong = saved.wrong || {};
        saved.wrong[picked[0]] = true;
      }
      saveState();
      render();
      fire(container, 'attempt');
      if (correct) { fire(container, 'correct'); }
      refreshAll();
    }

    buttons.forEach(function (button) {
      button.addEventListener('click', function (event) {
        event.preventDefault();
        if (button.classList.contains('disabled') || saved.correct) { return; }
        var choice = button.dataset.choice;
        if (multiple) {
          var picked = chosen();
          var index = picked.indexOf(choice);
          if (index >= 0) { picked.splice(index, 1); } else { picked.push(choice); }
          saved.chosen = picked;
        } else {
          saved.chosen = choice;
        }
        saveState();
        render();
      });
    });

    render();
    register(container, function () { return Boolean(saved.correct); }, function () { return Boolean(saved.checked); });
  }

  function sameSet(a, b) {
    if (a.length !== b.length) { return false; }
    var sortedA = a.slice().sort();
    var sortedB = b.slice().sort();
    return sortedA.every(function (value, index) { return value === sortedB[index]; });
  }

  // --- Escolha de palavra ---------------------------------------------------

  function setupWordChoices() {
    var containers = activity.querySelectorAll('.word-choice');
    for (var i = 0; i < containers.length; i++) { setupWordChoice(containers[i]); }
  }

  function setupWordChoice(container) {
    var id = container.id || ('word-' + Math.random());
    var saved = stateOf(id);
    var choices = Array.prototype.filter.call(container.children, function (child) {
      return child.classList.contains('choice');
    });
    if (!choices.length) { return; }
    var toggle = element('button', 'btn btn-secondary btn-sm dropdown-toggle');
    toggle.type = 'button';
    var menu = element('div', 'dropdown-menu');
    choices.forEach(function (choice) {
      var item = element('button', 'dropdown-item');
      item.type = 'button';
      item.innerHTML = choice.innerHTML;
      item.dataset.choice = choice.id;
      item.dataset.correct = choice.classList.contains('correct') ? '1' : '';
      item.addEventListener('click', function (event) {
        event.preventDefault();
        container.classList.remove('show');
        if (saved.correct) { return; }
        saved.chosen = choice.id;
        saved.attempts = (saved.attempts || 0) + 1;
        saved.correct = Boolean(item.dataset.correct);
        saveState();
        render();
        fire(container, 'attempt');
        if (saved.correct) { fire(container, 'correct'); }
        refreshAll();
      });
      menu.appendChild(item);
    });
    toggle.addEventListener('click', function (event) {
      event.preventDefault();
      if (saved.correct) { return; }
      container.classList.toggle('show');
    });
    document.addEventListener('click', function (event) {
      if (!container.contains(event.target)) { container.classList.remove('show'); }
    });
    container.appendChild(toggle);
    container.appendChild(menu);

    function render() {
      var picked = choices.filter(function (choice) { return choice.id === saved.chosen; })[0];
      toggle.innerHTML = picked ? picked.innerHTML : TEXT.chooseWord;
      toggle.className = 'btn btn-sm dropdown-toggle ' +
        (saved.correct ? 'btn-success btn-ximera-correct' : (picked ? 'btn-danger btn-ximera-incorrect' : 'btn-secondary btn-ximera-submit'));
      toggle.title = saved.correct ? TEXT.correct : (picked ? TEXT.tryAgain : TEXT.chooseWord);
    }

    render();
    register(container, function () { return Boolean(saved.correct); }, function () { return Boolean(saved.chosen); });
  }

  // --- Caixas de resposta ---------------------------------------------------

  function setupAnswers() {
    for (var id in answerSpecs) {
      var slot = document.getElementById(id);
      if (slot) { setupAnswer(slot, id, answerSpecs[id]); }
    }
    var inline = activity.querySelectorAll('.answer.respondable');
    for (var i = 0; i < inline.length; i++) {
      var span = inline[i];
      var spec = { answer: span.textContent.trim(), format: 'expression', given: false, antiderivative: false };
      var inlineId = span.id || ('answer-text-' + i);
      span.textContent = '';
      setupAnswer(span, inlineId, spec);
    }
  }

  function setupAnswer(slot, id, spec) {
    var saved = stateOf(id);
    slot.innerHTML = '';
    var box = element('span', 'xm-answer-box');
    var input = document.createElement('input');
    input.type = 'text';
    input.className = 'form-control';
    input.setAttribute('aria-label', TEXT.answerPlaceholder);
    input.autocomplete = 'off';
    input.spellcheck = false;
    input.value = saved.text || '';
    var submit = element('button', 'btn btn-primary btn-ximera-submit', '<i class="fa fa-fw fa-question"></i>');
    var correct = element('button', 'btn btn-success btn-ximera-correct', '<i class="fa fa-fw fa-check"></i>');
    var incorrect = element('button', 'btn btn-danger btn-ximera-incorrect', '<i class="fa fa-fw fa-times"></i>');
    submit.title = TEXT.check;
    correct.title = TEXT.correct;
    incorrect.title = TEXT.tryAgain;
    [submit, correct, incorrect].forEach(function (button) { button.type = 'button'; button.tabIndex = -1; });
    box.appendChild(input);
    box.appendChild(submit);
    box.appendChild(correct);
    box.appendChild(incorrect);
    slot.appendChild(box);

    function render() {
      var text = input.value.trim();
      var status = saved.correct && text === saved.checked ? 'correct'
        : (saved.checked !== undefined && text === saved.checked && text !== '' ? 'incorrect' : 'pending');
      submit.hidden = status !== 'pending';
      correct.hidden = status !== 'correct';
      incorrect.hidden = status !== 'incorrect';
      submit.disabled = text === '';
      input.readOnly = status === 'correct';
      box.classList.toggle('xm-correct', status === 'correct');
      box.classList.toggle('xm-incorrect', status === 'incorrect');
    }

    function check() {
      var text = input.value.trim();
      if (!text) { return; }
      saved.text = text;
      saved.checked = text;
      saved.attempts = (saved.attempts || 0) + 1;
      saved.correct = answerMatches(spec, text);
      saveState();
      render();
      fire(slot, 'attempt');
      if (saved.correct) { fire(slot, 'correct'); }
      refreshAll();
    }

    input.addEventListener('input', function () {
      saved.text = input.value;
      saveState();
      render();
    });
    input.addEventListener('keydown', function (event) {
      if (event.key === 'Enter') { event.preventDefault(); check(); }
    });
    submit.addEventListener('click', check);
    incorrect.addEventListener('click', check);
    render();
    register(slot, function () { return Boolean(saved.correct) && (saved.text || '').trim() === saved.checked; },
      function () { return saved.checked !== undefined; });
  }

  function answerMatches(spec, text) {
    var expected = spec.answer;
    if (spec.format === 'string') {
      return normalizeText(text) === normalizeText(expected);
    }
    if (spec.format === 'integer' || spec.format === 'float') {
      var given = Number(text.replace(',', '.'));
      var wanted = numberOf(expected);
      if (isNaN(given) || wanted === null) { return false; }
      return withinTolerance(given, wanted, spec.tolerance);
    }
    var library = window.MathExpressions;
    if (library && !library.fromText && library.default) { library = library.default; }
    if (library) {
      try {
        var correct = library.fromLatex(expected);
        var student = parseStudent(library, text);
        if (spec.antiderivative) {
          return correct.derivative('x').equals(student.derivative('x'));
        }
        if (spec.tolerance !== undefined && spec.tolerance !== null) {
          var a = correct.evaluate_to_constant();
          var b = student.evaluate_to_constant();
          if (typeof a === 'number' && typeof b === 'number') { return withinTolerance(b, a, spec.tolerance); }
        }
        return correct.equals(student);
      } catch (error) {
        /* cai na comparação simples */
      }
    }
    var wantedNumber = numberOf(expected);
    var givenNumber = numberOf(text);
    if (wantedNumber !== null && givenNumber !== null) { return withinTolerance(givenNumber, wantedNumber, spec.tolerance); }
    return normalizeText(text) === normalizeText(expected);
  }

  function parseStudent(library, text) {
    try { return library.fromText(text); } catch (error) { return library.fromLatex(text); }
  }

  function withinTolerance(given, wanted, tolerance) {
    var limit = tolerance ? Number(tolerance) : Math.max(1e-9, Math.abs(wanted) * 1e-6);
    return Math.abs(given - wanted) <= limit;
  }

  function numberOf(text) {
    var cleaned = String(text).replace(/\\[,;!]|\s|\\left|\\right|[{}$]/g, '').replace(',', '.');
    cleaned = cleaned.replace(/^\\frac(-?\d+(?:\.\d+)?)(-?\d+(?:\.\d+)?)$/, '$1/$2');
    if (/^-?\d+(?:\.\d+)?\/-?\d+(?:\.\d+)?$/.test(cleaned)) {
      var parts = cleaned.split('/');
      return Number(parts[0]) / Number(parts[1]);
    }
    if (/^-?\d+(?:\.\d+)?$/.test(cleaned)) { return Number(cleaned); }
    return null;
  }

  function normalizeText(text) {
    return String(text).replace(/\\text\s*\{([^}]*)\}/g, '$1').replace(/[\s{}$]/g, '').toLowerCase();
  }

  // --- Resposta livre -------------------------------------------------------

  function setupFreeResponses() {
    var containers = activity.querySelectorAll('.free-response');
    for (var i = 0; i < containers.length; i++) {
      (function (container) {
        var id = container.id || ('free-' + i);
        var saved = stateOf(id);
        var textarea = document.createElement('textarea');
        textarea.className = 'form-control';
        textarea.rows = 5;
        textarea.placeholder = TEXT.freeResponse;
        textarea.value = saved.text || '';
        textarea.addEventListener('input', function () {
          saved.text = textarea.value;
          saveState();
          refreshAll();
        });
        container.appendChild(textarea);
        register(container, function () { return Boolean((saved.text || '').trim()); }, function () { return Boolean((saved.text || '').trim()); });
      })(containers[i]);
    }
  }

  // --- Dicas ----------------------------------------------------------------

  function setupHints() {
    var accordions = Array.prototype.filter.call(activity.querySelectorAll('.accordion'), function (accordion) {
      return accordion.querySelector('.xmhint-content');
    });
    var groups = [];
    accordions.forEach(function (accordion) {
      accordion.classList.add('xm-hint', 'xm-hint-hidden');
      var owner = closestProblem(accordion);
      var group = groups.filter(function (candidate) { return candidate.owner === owner; })[0];
      if (!group) {
        group = { owner: owner, hints: [] };
        groups.push(group);
      }
      group.hints.push(accordion);
    });
    groups.forEach(setupHintGroup);
  }

  function setupHintGroup(group) {
    var id = (group.owner.id || 'activity') + ':hints';
    var saved = stateOf(id);
    var button = element('button', 'btn btn-info btn-reveal-hint');
    button.type = 'button';
    button.title = TEXT.revealHint;
    group.hints[0].parentNode.insertBefore(button, group.hints[0]);
    group.hints.forEach(function (hint, index) {
      var header = element('div', 'xm-hint-header');
      var label = element('span', 'xm-hint-label', TEXT.hint + ' ' + (index + 1));
      var collapse = element('button', 'btn btn-info btn-xs btn-hint-collapse', '<i class="fa fa-chevron-down"></i>');
      collapse.type = 'button';
      collapse.setAttribute('aria-label', TEXT.hint);
      collapse.addEventListener('click', function () {
        hint.classList.toggle('xm-hint-collapsed');
        collapse.firstChild.classList.toggle('fa-rotate-90', hint.classList.contains('xm-hint-collapsed'));
      });
      header.appendChild(collapse);
      header.appendChild(label);
      hint.insertBefore(header, hint.firstChild);
    });

    function render() {
      var revealed = saved.revealed || 0;
      group.hints.forEach(function (hint, index) {
        hint.classList.toggle('xm-hint-hidden', index >= revealed);
      });
      button.hidden = revealed >= group.hints.length;
      button.innerHTML = '<i class="fa fa-life-ring"></i>&nbsp; ' + TEXT.revealHint +
        (group.hints.length > 1 ? ' (' + (Math.min(revealed, group.hints.length - 1) + 1) + ' de ' + group.hints.length + ')' : '');
    }

    button.addEventListener('click', function () {
      saved.revealed = Math.min((saved.revealed || 0) + 1, group.hints.length);
      saveState();
      render();
      if (window.MathJax && MathJax.typesetPromise) { MathJax.typesetPromise([group.hints[saved.revealed - 1]]).catch(function () {}); }
    });
    render();
  }

  // --- Comentários (feedback) e ambientes aninhados --------------------------

  function register(node, isCorrect, isAttempted) {
    problems.push({ node: node, isCorrect: isCorrect, isAttempted: isAttempted });
  }

  function answerablesWithin(container) {
    return problems.filter(function (problem) { return container.contains(problem.node); });
  }

  function directAnswerables(container) {
    return problems.filter(function (problem) {
      return container.contains(problem.node) && closestProblem(problem.node) === container;
    });
  }

  function refreshFeedback() {
    var feedbacks = activity.querySelectorAll('.feedback');
    for (var i = 0; i < feedbacks.length; i++) {
      var feedback = feedbacks[i];
      var owner = closestProblem(feedback);
      var inside = answerablesWithin(owner);
      var kind = feedback.getAttribute('data-feedback') || 'attempt';
      var available = kind === 'correct'
        ? inside.some(function (problem) { return problem.isCorrect(); })
        : inside.some(function (problem) { return problem.isAttempted(); });
      feedback.classList.toggle('xm-available', available);
    }
  }

  // Um ambiente aninhado (pergunta, explicação...) só aparece depois que as
  // respostas diretas do ambiente que o contém estão certas.
  function refreshGating() {
    var containers = activity.querySelectorAll('.problem-environment');
    for (var i = 0; i < containers.length; i++) {
      var container = containers[i];
      var direct = directAnswerables(container);
      var complete = !direct.length || direct.every(function (problem) { return problem.isCorrect(); });
      var children = container.querySelectorAll('.problem-environment');
      for (var j = 0; j < children.length; j++) {
        if (closestProblem(children[j]) === container) {
          children[j].classList.toggle('xm-hidden', !complete);
        }
      }
    }
  }

  function refreshProgress() {
    if (!problems.length) { return; }
    var done = problems.filter(function (problem) { return problem.isCorrect(); }).length;
    var percent = Math.round(100 * done / problems.length);
    var card = document.querySelector('.activity-card.active .progress-bar');
    if (card) { card.style.width = percent + '%'; }
    try {
      var progress = JSON.parse(localStorage.getItem(progressKey) || '{}') || {};
      progress[activity.getAttribute('data-path') || location.pathname] = percent;
      localStorage.setItem(progressKey, JSON.stringify(progress));
    } catch (error) { /* ignora */ }
  }

  function refreshAll() {
    refreshFeedback();
    refreshGating();
    refreshProgress();
  }

  function showStoredProgress() {
    var progress;
    try { progress = JSON.parse(localStorage.getItem(progressKey) || '{}') || {}; } catch (error) { return; }
    var cards = document.querySelectorAll('.activity-card[data-file]');
    for (var i = 0; i < cards.length; i++) {
      var link = cards[i].querySelector('a');
      var bar = cards[i].querySelector('.progress-bar');
      if (!link || !bar) { continue; }
      var path = link.getAttribute('data-path');
      if (path && progress[path] !== undefined) { bar.style.width = progress[path] + '%'; }
    }
  }

  function setupGraphs() {
    var graphs = activity.querySelectorAll('.graph');
    for (var i = 0; i < graphs.length; i++) { graphs[i].textContent = graphs[i].textContent.trim(); }
  }

  function setupReset() {
    var buttons = document.querySelectorAll('.xm-reset');
    for (var i = 0; i < buttons.length; i++) {
      buttons[i].addEventListener('click', function () {
        if (!window.confirm(TEXT.resetConfirm)) { return; }
        state = {};
        saveState();
        location.reload();
      });
    }
  }

  function scrollStripToCurrent() {
    var current = document.querySelector('.course-navigation .activity-card.active');
    if (current && current.parentNode) {
      var strip = current.parentNode;
      strip.scrollLeft = Math.max(0, current.offsetLeft - strip.clientWidth / 2 + current.offsetWidth / 2);
    }
  }

  function init() {
    setupAnswers();
    setupChoices('.multiple-choice', false);
    setupChoices('.select-all', true);
    setupWordChoices();
    setupFreeResponses();
    setupHints();
    setupGraphs();
    setupReset();
    refreshAll();
    showStoredProgress();
    scrollStripToCurrent();
    activity.classList.add('xm-ready');
  }

  function start() {
    if (window.MathJax && MathJax.startup && MathJax.startup.promise) {
      MathJax.startup.promise.then(init, init);
    } else {
      init();
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', start);
  } else {
    start();
  }
})();
