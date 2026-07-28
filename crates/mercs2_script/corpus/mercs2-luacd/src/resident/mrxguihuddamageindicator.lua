import("MrxGui")

function HandleReceiveDamageEvent(oWidget, nDamageDirection, nDamageAmount)
  if not oWidget.CustomData.bInitialized then
    oWidget.CustomData.bInitialized = true
    local oParent = oWidget.ParentWidget
    local oReticle = MrxGui.GetWidgetByNameAndOwner("reticle", oWidget:GetOwner())
    if oReticle and oParent then
      local nReticleX1, nReticleY1, nReticleX2, nReticleY2 = oReticle:GetLocation()
      local nCenterX = (nReticleX1 + nReticleX2) * 0.5
      local nCenterY = (nReticleY1 + nReticleY2) * 0.5
      local nX1, nY1, nX2, nY2 = oParent:GetLocation()
      local nWidth = nX2 - nX1
      local nHeight = nY2 - nY1
      oParent:SetLocation(nCenterX - nWidth * 0.5, nCenterY - nHeight * 0.5)
    end
  end
  nDamageAmount = nDamageAmount or 20
  if nDamageAmount <= 0 then
    return
  end
  local uPlayer = oWidget:GetOwner()
  if uPlayer then
    local uObject = Player.GetControlledObject(uPlayer)
    if uObject then
      local nMaxHealth = Object.GetMaxHealth(uObject)
      if nMaxHealth then
        nDamageAmount = nDamageAmount * 100 / nMaxHealth
      end
    end
  end
  if not oWidget.CustomData.nDamageAmount then
    oWidget.CustomData.nDamageAmount = 0
  end
  local oNewIndicator = MrxGui.ImageWidget:new()
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  oNewIndicator:SetTexture(oWidget:GetTexture())
  oNewIndicator.CustomData.nDamageDirection = nDamageDirection
  oNewIndicator.ParentWidget = oWidget
  oNewIndicator:SetOwner(oWidget:GetOwner())
  oNewIndicator:SetEventHandler("GuiUpdate", HandleUpdateEvent)
  local nCameraRotation = Player.GetCameraXZHeading(oWidget:GetOwner())
  oNewIndicator:SetRotation(nCameraRotation - oNewIndicator.CustomData.nDamageDirection)
  oNewIndicator:SetLocation(nX1, nY1, nX2, nY2)
  oNewIndicator:SetAnchoring("center", "center")
  oNewIndicator:SetTranslucency(math.min(math.pow(nDamageAmount, 0.5) * 100, 255))
  MrxGui.AddWidget(oNewIndicator)
end

function _Finish(oWidget)
  oWidget.CustomData.nDamageAmount = 0
  oWidget:SetVisible(false)
end

function HandleUpdateEvent(oWidget, tEvent)
  local nCameraRotation = Player.GetCameraXZHeading(oWidget:GetOwner())
  oWidget:SetRotation(oWidget.CustomData.nDamageDirection + nCameraRotation)
  local nAlpha = oWidget:GetTranslucency()
  nAlpha = nAlpha - 100 * tEvent
  if 0 < nAlpha then
    oWidget:SetTranslucency(nAlpha)
  else
    DeleteDamageIndicatorCallback(oWidget)
  end
end

function DeleteDamageIndicatorCallback(oWidget)
  MrxGui.RemoveWidget(oWidget)
  oWidget:delete()
end

function HandleE3HudModeEvent(oWidget, tEvent)
  if tEvent.bOn then
    if oWidget.EventHandlers.GuiPlayerReceiveDamage then
      oWidget:SetEventHandler("GuiPlayerReceiveDamage", nil)
    end
  elseif not oWidget.EventHandlers.GuiPlayerReceiveDamage then
    oWidget:SetEventHandler("GuiPlayerReceiveDamage", HandleReceiveDamageEvent)
  end
end
