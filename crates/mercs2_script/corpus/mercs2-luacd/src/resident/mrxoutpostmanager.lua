import("MrxUtil")
knStatusCaptured = 1
knStatusDestroyed = 2
_tOutposts = {}

function RegisterOutpost(uOutpost)
  if _tOutposts[uOutpost] then
    return
  end
  _tOutposts[uOutpost] = {
    tCallbacks = {}
  }
end

function UnregisterOutpost(uOutpost)
  if not _tOutposts[uOutpost] then
    return
  end
  _tOutposts[uOutpost] = nil
end

function RegisterOutpostEvent(uOutpost, fCallback, tCallbackArgs)
  RegisterOutpost(uOutpost)
  table.insert(_tOutposts[uOutpost].tCallbacks, {fCallback = fCallback, tCallbackArgs = tCallbackArgs})
end

function OutpostStatusChange(uOutpost, nStatus)
  if not _tOutposts[uOutpost] then
    return
  end
  for i, tCallback in ipairs(_tOutposts[uOutpost].tCallbacks) do
    Debug.Printf("Issuing status change callback for outpost (status: " .. nStatus .. ")")
    MrxUtil.CallWithOptionalArgs(tCallback.fCallback, {
      unpack(tCallback.tCallbackArgs),
      uOutpost,
      nStatus
    })
  end
  UnregisterOutpost(uOutpost)
end
