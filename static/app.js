(() => {
  const form = document.querySelector('#translation-form');
  if (!form) return;
  const field = form.elements.text, status = document.querySelector('#save-status');
  const id = form.closest('[data-segment-id]').dataset.segmentId;
  let timer, controller;
  const set = t => { status.textContent = t; };
  async function save() {
    clearTimeout(timer); if (controller) controller.abort(); controller = new AbortController(); set('Salvando…');
    try { const r = await fetch(`/segments/${id}/draft`, {method:'PUT',body:new URLSearchParams({text:field.value}),headers:{'Content-Type':'application/x-www-form-urlencoded'},signal:controller.signal}); if (!r.ok) throw new Error(await r.text()); set('Salvo'); }
    catch (e) { if (e.name !== 'AbortError') set(`Erro: ${e.message}. Tente novamente.`); }
  }
  field.addEventListener('input', () => { clearTimeout(timer); set('Aguardando…'); timer=setTimeout(save,1500); });
  field.addEventListener('blur', save);
  document.querySelector('#copy-original').addEventListener('click', () => {
    const original = document.querySelector('.reading').textContent;
    if (field.value && field.value !== original && !confirm('Substituir o rascunho atual pelo original?')) return;
    field.value = original;
    field.focus();
    field.dispatchEvent(new Event('input', {bubbles:true}));
  });
  document.addEventListener('keydown', e => {
    if (!e.ctrlKey) return;
    if(e.key==='Enter'){e.preventDefault();form.requestSubmit()}
    if(e.key.toLowerCase()==='s'){e.preventDefault();save()}
    const writing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target.isContentEditable;
    if (!writing && e.key==='ArrowLeft'){location=`/segments/${id}/previous`}
    if (!writing && e.key==='ArrowRight'){location=`/segments/${id}/next`}
  });
})();
