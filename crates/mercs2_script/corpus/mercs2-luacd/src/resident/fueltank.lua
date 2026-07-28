function OnStateChange(uiGuid, uiNodeHashName, uiStateHashName)
  local sStateHashName = Sys.GuidToString(uiStateHashName)
  
  if sStateHashName == "0x7687DF41" then
    local fxName = ObjectState.GetStringHash("fx_EmitFlameOilrigTower")
    ObjectState.StartEmitter(uiGuid, uiNodeHashName, fxName)
    local newTime = Math.randf(12, 20)
    Event.Create(Event.TimerRelative, {newTime}, _StartSmoke, {
      uiGuid,
      uiNodeHashName,
      fxName
    })
  end
end

function _StartSmoke(uiGuid, uiNodeHashName, fxName)
  ObjectState.StopEmitter(uiGuid, uiNodeHashName, fxName)
  ObjectState.StartEmitter(uiGuid, uiNodeHashName, ObjectState.GetStringHash("fx_EmitSmokeStack"))
end
