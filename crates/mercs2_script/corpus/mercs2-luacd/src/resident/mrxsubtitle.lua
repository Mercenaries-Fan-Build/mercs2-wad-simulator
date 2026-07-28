_tMsgIds = {}
_knDisplayDuration = 5
_knFadeDuration = 0.5

function Add(vMsgs, fCallback, tCallbackArgs)
  local tMsgs
  local sType = type(vMsgs)
  if sType == "string" then
    tMsgs = {vMsgs}
  elseif sType == "table" then
    tMsgs = vMsgs
  else
    return
  end
  local nMsgs = table.getn(tMsgs)
  for i, sMsg in ipairs(tMsgs) do
    local tConfig = {
      sMessage = sMsg,
      nDuration = _knDisplayDuration,
      nFadeTime = _knFadeDuration,
      bClearBuffer = true,
      bAllowsAppends = false
    }
    if i == nMsgs then
      tConfig.fCallback = fCallback
      tConfig.tCallbackData = tCallbackArgs
    end
    local tMsgId = Hud.SubtitleBuffer:AddMessage(tConfig)
    if tMsgId then
      table.insert(_tMsgIds, tMsgId)
    end
  end
end

function ClearPending()
  for i, tMsgId in ipairs(_tMsgIds) do
    Hud.SubtitleBuffer:RemovePendingMessage({tMessageIds = tMsgId})
  end
  _tMsgIds = nil
  _tMsgIds = {}
end
