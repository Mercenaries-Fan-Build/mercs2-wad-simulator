import("MrxGui")

function SetCounterMessageVisible(bShow, uPlayerGuid)
end

function SetMeleeMessage(sString, uPlayerGuid)
end

function DisplayCounterMessage(nDisplayTime, uPlayerGuid)
end

function HandleUpdateEvent(oWidget, nTime)
end

function HandleInitializationEvent(oWidget, oEvent)
end

function HideOnComplete(oWidget)
end

function SetContextActionMessage(sText, uPlayer, nPriority)
  local oContextActionWidget
  if "userdata" ~= type(uPlayer) then
    oContextActionWidget = MrxGui.GetWidgetByName("Context Action Text")
  else
    oContextActionWidget = MrxGui.GetWidgetByNameAndOwner("Context Action Text", uPlayer)
  end
  if not oContextActionWidget then
    return
  end
  nPriority = nPriority or 1
  if not oContextActionWidget.CustomData.bInitialized then
    _Initialize(oContextActionWidget)
  end
  if not sText then
    oContextActionWidget.CustomData.tMessageQueue[nPriority] = nil
    if nPriority == oContextActionWidget.CustomData.nCurrentPriority then
      local nNewPriority = 0
      local sNewText
      for nCheckPriority, sCheckText in pairs(oContextActionWidget.CustomData.tMessageQueue) do
        nNewPriority = nCheckPriority
        sNewText = sCheckText
      end
      if sNewText then
        if not oContextActionWidget.CustomData.sCurrentText or oContextActionWidget.CustomData.sCurrentText ~= sNewText then
          Sound.CueSound(0, "ui_HUD_Contextual_Action_Alert")
        end
        oContextActionWidget:SetText("[action] " .. sNewText)
        oContextActionWidget:AnimateToPoint(oContextActionWidget.CustomData.nVisiblePoint, 0, true)
        oContextActionWidget.CustomData.nCurrentPriority = nNewPriority
        oContextActionWidget.CustomData.sCurrentText = sText
      else
        oContextActionWidget:AnimateToPoint(oContextActionWidget.CustomData.nFadePoint, 1, true, ContextActionWidgetRemovalCallback)
        oContextActionWidget.CustomData.sCurrentText = nil
      end
    end
  else
    oContextActionWidget.CustomData.tMessageQueue[nPriority] = sText
    if not oContextActionWidget.CustomData.sCurrentText or oContextActionWidget.CustomData.sCurrentText ~= sText then
      Sound.CueSound(0, "ui_HUD_Contextual_Action_Alert")
    end
    oContextActionWidget:SetVisible(true)
    oContextActionWidget:SetText("[action] " .. sText)
    oContextActionWidget.CustomData.nCurrentPriority = nPriority
    oContextActionWidget.CustomData.sCurrentText = sText
    oContextActionWidget:AnimateToPoint(oContextActionWidget.CustomData.nVisiblePoint, 0, true)
  end
end

function ContextActionWidgetRemovalCallback(oContextActionWidget)
  oContextActionWidget:SetVisible(false)
  oContextActionWidget:SetText("")
  oContextActionWidget.CustomData.nCurrentPriority = 0
  oContextActionWidget:SetTranslucency(255)
end

function _Initialize(oWidget)
  oWidget.CustomData.bInitialized = true
  oWidget.CustomData.tMessageQueue = {}
  oWidget.CustomData.nCurrentPriority = 0
  oWidget.CustomData.nVisiblePoint = oWidget:AddAnimationPoint({TranslucencyLevel = 255})
  oWidget.CustomData.nFadePoint = oWidget:AddAnimationPoint({TranslucencyLevel = 0})
end
