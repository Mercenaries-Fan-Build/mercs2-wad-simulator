import("MrxGuiBase")
_knPulseTime = 0.4

function Min(nA, nB)
  if nA < nB then
    return nA
  else
    return nB
  end
end

function HandleCurrentGunAmmoUpdateEvent(oWidget, tEvent)
  if "number" == type(tEvent.PrimaryCurrentAmmo) and tEvent.PrimaryCurrentAmmo ~= -1 and "number" == type(tEvent.PrimaryClipSize) and tEvent.PrimaryClipSize ~= -1 and tEvent.PrimaryClipSize ~= 0 then
    oWidget:SetText(tEvent.PrimaryCurrentAmmo)
    local tData = oWidget.CustomData
    if tData.nPreviousValue and tEvent.PrimaryClipSize then
      if not tData.nRedPoint then
        tData.nRedPoint = oWidget:AddAnimationPoint({
          RedLevel = 216,
          GreenLevel = 16,
          BlueLevel = 16
        })
      end
      if not tData.nNeutralPoint then
        local nR, nG, nB = oWidget:GetColor()
        tData.nNeutralPoint = oWidget:AddAnimationPoint({
          RedLevel = nR,
          GreenLevel = nG,
          BlueLevel = nB
        })
      end
      local nThreshold = tEvent.PrimaryClipSize / 3
      local nValue = tEvent.PrimaryCurrentAmmo
      if not tData.bAnimating then
        if nThreshold > nValue then
          _PulseToRed(oWidget, _knPulseTime)
          tData.bAnimating = true
          Event.Post("Ammo low", {
            uPlayer = oWidget:GetOwner()
          })
        end
      elseif nThreshold <= nValue then
        oWidget:AnimateToPoint(tData.nNeutralPoint, 0.4, true)
        tData.bAnimating = false
        Event.Post("Ammo not low", {
          uPlayer = oWidget:GetOwner()
        })
      end
    end
    tData.nPreviousValue = tEvent.PrimaryCurrentAmmo
  else
    oWidget:SetText(" ")
  end
end

function _PulseToRed(oWidget, nSpeed)
  if oWidget.CustomData.nRedPoint then
    oWidget:AnimateToPoint(oWidget.CustomData.nRedPoint, nSpeed, true, _PulseToNeutral, {nSpeed})
  end
end

function _PulseToNeutral(oWidget, nSpeed)
  if oWidget.CustomData.nNeutralPoint then
    oWidget:AnimateToPoint(oWidget.CustomData.nNeutralPoint, nSpeed, true, _PulseToRed, {nSpeed})
  end
end

function HandleCurrentGunClipSizeUpdateEvent(oWidget, tEvent)
  if "number" == type(tEvent.PrimaryClipSize) and tEvent.PrimaryClipSize ~= -1 and tEvent.PrimaryClipSize ~= 0 then
    oWidget:SetText("/" .. tEvent.PrimaryClipSize)
  else
    oWidget:SetText(" ")
  end
end

function HandleStoredGunAmmoUpdateEvent(oWidget, tEvent)
  if "number" == type(tEvent.PrimaryStoredAmmo) and tEvent.PrimaryStoredAmmo ~= -1 and "number" == type(tEvent.PrimaryClipSize) and tEvent.PrimaryClipSize ~= -1 and tEvent.PrimaryClipSize ~= 0 then
    oWidget:SetText(tEvent.PrimaryStoredAmmo)
  else
    oWidget:SetText(" ")
  end
end

function HandleUnreloadableGunAmmoUpdateEvent(oWidget, tEvent)
  if tEvent.PrimaryCurrentAmmo and tEvent.PrimaryClipSize then
    if -1 ~= tEvent.PrimaryCurrentAmmo and -1 == tEvent.PrimaryClipSize then
      oWidget:SetText(tEvent.PrimaryCurrentAmmo)
    elseif -1 ~= tEvent.PrimaryCurrentAmmo and 0 == tEvent.PrimaryClipSize then
      oWidget:SetText(tEvent.PrimaryCurrentAmmo)
    else
      oWidget:SetText(" ")
    end
  end
end

function HandleExplosivesAmmoUpdateEvent(oWidget, tEvent)
  if "number" == type(tEvent.ExplosivesStoredAmmo) and "number" == type(tEvent.ExplosivesCurrentAmmo) and tEvent.ExplosivesStoredAmmo ~= -1 and tEvent.ExplosivesCurrentAmmo ~= -1 then
    local nTotal = tEvent.ExplosivesCurrentAmmo + tEvent.ExplosivesStoredAmmo
    oWidget:SetText(nTotal)
    local tData = oWidget.CustomData
    if not tData.nRedPoint then
      tData.nRedPoint = oWidget:AddAnimationPoint({
        RedLevel = 216,
        GreenLevel = 16,
        BlueLevel = 16
      })
    end
    if not tData.nNeutralPoint then
      local nR, nG, nB = oWidget:GetColor()
      tData.nNeutralPoint = oWidget:AddAnimationPoint({
        RedLevel = nR,
        GreenLevel = nG,
        BlueLevel = nB
      })
    end
    if nTotal <= 0 then
      if not tData.bAnimating then
        _PulseToRed(oWidget, _knPulseTime)
        tData.bAnimating = true
      end
    elseif tData.bAnimating then
      oWidget:AnimateToPoint(tData.nNeutralPoint, _knPulseTime, true)
      tData.bAnimating = false
    end
  else
    oWidget:SetText(" ")
  end
end

function HandleTopLevelUpdateEvent(oWidget, nDeltaTime)
  if oWidget.CustomData.nVisibilityTime > 0 then
    oWidget.CustomData.nRemainingVisibleTime = oWidget.CustomData.nRemainingVisibleTime - nDeltaTime
    if 0 >= oWidget.CustomData.nRemainingVisibleTime then
      oWidget.CustomData.nRemainingVisibleTime = oWidget.CustomData.nVisibilityTime
      oWidget.CustomData.oCounter:AnimateToPoint(oWidget.CustomData.nFadePoint, nil, true, oWidget.CustomData.oCounter.SetVisible, {false})
      oWidget:SetEventHandler("GuiUpdate", nil)
    end
  end
end

function HandleTopLevelGunAmmoUpdateEvent(oWidget, tEvent)
  if "number" == type(tEvent.PrimaryStoredAmmo) and tEvent.PrimaryStoredAmmo ~= -1 and tEvent.PrimaryStoredAmmo ~= oWidget.CustomData.nCachedStoredAmmo or "number" == type(tEvent.PrimaryCurrentAmmo) and tEvent.PrimaryCurrentAmmo ~= -1 and tEvent.PrimaryCurrentAmmo ~= oWidget.CustomData.nCachedClipAmmo then
    oWidget.CustomData.nCachedClipAmmo = tEvent.PrimaryCurrentAmmo or oWidget.CustomData.nCachedClipAmmo
    oWidget.CustomData.nCachedClipSize = tEvent.PrimaryClipSize or oWidget.CustomData.nCachedClipSize
    oWidget.CustomData.nCachedStoredAmmo = tEvent.PrimaryStoredAmmo
    _ShowForDuration(oWidget)
  end
end

function HandleTopLevelExplosiveAmmoUpdateEvent(oWidget, tEvent)
  if "number" == type(tEvent.ExplosivesStoredAmmo) and "number" == type(tEvent.ExplosivesCurrentAmmo) and tEvent.ExplosivesStoredAmmo ~= -1 and tEvent.ExplosivesCurrentAmmo ~= -1 and (tEvent.ExplosivesStoredAmmo ~= oWidget.CustomData.nCachedStoredAmmo or tEvent.ExplosivesCurrentAmmo ~= oWidget.CustomData.nCachedClipAmmo) then
    oWidget.CustomData.nCachedClipAmmo = tEvent.ExplosivesCurrentAmmo
    oWidget.CustomData.nCachedStoredAmmo = tEvent.ExplosivesStoredAmmo
    _ShowForDuration(oWidget)
  end
end

function HandleTopLevelInitialization(oWidget, tEvent)
  local tChildren = oWidget:GetChildren()
  local oCurrentGun = tChildren[1]
  oCurrentGun.TriggerAnimation = _WeaponSwitchAccessor
  oCurrentGun.SetSuppressAnimation = _SetSuppressAnimation
  local oIcon = oCurrentGun:GetChildren()[1]:GetChildren()[1]:GetChildren()[1]
  MrxGuiBase.PushWidgetToFront(oIcon)
  oWidget.CustomData.oCounter = tChildren[1]
  if tChildren[2] then
    local oName = tChildren[2]
    oName.CustomData.nFadePoint = oName:AddAnimationPoint({TranslucencyLevel = 0})
    oName.CustomData.nVisiblePoint = oName:AddAnimationPoint({TranslucencyLevel = 255})
    oWidget.CustomData.oName = oName
    oName:SetText(" ")
    oName:AnimateToPoint(oName.CustomData.nFadePoint, 0, true)
  end
  _SetUpFadeBehavior(oWidget)
end

function _SetSuppressAnimation(oWidget, bSuppress)
  oWidget.CustomData.bSuppress = bSuppress
end

function HandleGunShowEvent(oWidget, tEvent)
  if tEvent.bShowGun then
    local nDuration
    if "number" == type(tEvent.nTime) then
      nDuration = tEvent.nTime
    end
    _ShowForDuration(oWidget, nDuration)
  end
end

function HandleExplosiveShowEvent(oWidget, tEvent)
  if tEvent.bShowExplosive then
    local nDuration
    if "number" == type(tEvent.nTime) then
      nDuration = tEvent.nTime
    end
    _ShowForDuration(oWidget, nDuration)
  end
end

function _ShowForDuration(oWidget, nDuration)
  if not oWidget.CustomData.bHaveWeapon then
    return
  end
  if oWidget.CustomData.nCachedClipAmmo and oWidget.CustomData.nCachedClipSize and (oWidget.CustomData.nCachedClipAmmo < oWidget.CustomData.nCachedClipSize / 3 or oWidget.CustomData.nCachedClipAmmo <= 0) then
    nDuration = -1
  end
  oWidget.CustomData.oCounter:SetTranslucency(255)
  oWidget.CustomData.oCounter:AnimateToPoint(oWidget.CustomData.nVisiblePoint, 0, true)
  oWidget:SetVisible(true)
  if not nDuration then
    if 0 < oWidget.CustomData.nVisibilityTime then
      oWidget.CustomData.nRemainingVisibleTime = oWidget.CustomData.nVisibilityTime
      oWidget:SetEventHandler("GuiUpdate", HandleTopLevelUpdateEvent)
    end
  elseif 0 == nDuration then
    oWidget:SetEventHandler("GuiUpdate", nil)
  elseif 0 < nDuration then
    oWidget.CustomData.nRemainingVisibleTime = nDuration
    oWidget:SetEventHandler("GuiUpdate", HandleTopLevelUpdateEvent)
  else
    oWidget:SetEventHandler("GuiUpdate", nil)
  end
end

function HandleGunSwitchEvent(oWidget, tEvent)
  if tEvent.uNewCurrentGun then
    if "string" == type(tEvent.uNewCurrentGun) or "userdata" == type(tEvent.uNewCurrentGun) then
      oWidget.CustomData.bHaveWeapon = true
      _ShowForDuration(oWidget)
      if tEvent.uNewCurrentGunGuid then
        local sName = Object.GetLocalizedName(tEvent.uNewCurrentGunGuid)
        if sName then
          local oName = oWidget.CustomData.oName
          oName:SetText(sName)
          oName:AnimateToPoint(oName.CustomData.nVisiblePoint, 0, true, _NameDelay, {})
        end
      end
    else
      oWidget.CustomData.bHaveWeapon = false
    end
  end
end

function _NameDelay(oName)
  oName:AnimateToPoint(oName.CustomData.nVisiblePoint, 1, true, oName.AnimateToPoint, {
    oName.CustomData.nFadePoint,
    1,
    true
  })
end

function HandleExplosiveSwitchEvent(oWidget, tEvent)
  if tEvent.uNewCurrentExplosive then
    if "string" == type(tEvent.uNewCurrentExplosive) or "userdata" == type(tEvent.uNewCurrentExplosive) then
      oWidget.CustomData.bHaveWeapon = true
      _ShowForDuration(oWidget)
    else
      oWidget.CustomData.bHaveWeapon = false
    end
  end
end

function FindEquippedSupportTexture(uPlayer)
  return nil
end

function HandleE3HudModeEvent(oWidget, tEvent)
  if tEvent.bOn then
    oWidget.CustomData.bE3HudMode = true
    MrxGuiBase.RemoveWidgetWithChildren(oWidget:GetChildren()[1])
    MrxGuiBase.RemoveWidgetWithChildren(oWidget:GetChildren()[2])
  elseif oWidget.CustomData.bE3HudMode then
    oWidget.CustomData.bE3HudMode = nil
    MrxGuiBase.AddWidgetWithChildren(oWidget:GetChildren()[1])
    MrxGuiBase.AddWidgetWithChildren(oWidget:GetChildren()[2])
  end
end

function _SetUpFadeBehavior(oWidget)
  oWidget.CustomData.nFadePoint = oWidget.CustomData.oCounter:AddAnimationPoint({TranslucencyLevel = 0, nAnimationTime = 1})
  oWidget.CustomData.nVisiblePoint = oWidget.CustomData.oCounter:AddAnimationPoint({TranslucencyLevel = 255})
  oWidget.CustomData.nVisibilityTime = 3
  oWidget.CustomData.nRemainingVisibleTime = oWidget.CustomData.nVisibilityTime
  oWidget.CustomData.nCachedClipAmmo = 0
  oWidget.CustomData.nCachedStoredAmmo = 0
end

function _GreenFade(oWidget)
  if not oWidget.CustomData.nNormalColorPoint then
    oWidget.CustomData.nNormalColorPoint = oWidget:AddAnimationPoint({
      RedLevel = 255,
      GreenLevel = 255,
      BlueLevel = 255
    })
  end
  oWidget:SetColor(0, 216, 0)
  oWidget:AnimateToPoint(oWidget.CustomData.nNormalColorPoint, 2, true)
end

function _PerformIconSwitchAnimation(oWidget, uNewCurrentGun)
  oWidget.CustomData.uNewTexture = uNewCurrentGun
  if not oWidget.CustomData.bHavePoints then
    _SetUpFlippingPoints(oWidget)
    MrxGuiBase.PushWidgetToFront(oWidget:GetChildren()[1])
  end
  oWidget:AnimateToPoint(oWidget.CustomData.nClosePoint, 0.15, true, _SwitchTexture, {})
end

function _SwitchTexture(oWidget)
  if "userdata" == type(oWidget.CustomData.uNewTexture) or "string" == type(oWidget.CustomData.uNewTexture) then
    oWidget:GetChildren()[1]:SetTexture(oWidget.CustomData.uNewTexture)
    oWidget:GetChildren()[1]:SetVisible(true)
    oWidget:SetVisible(true)
    oWidget:AnimateToPoint(oWidget.CustomData.nStartPoint, 0.15, true)
  else
    oWidget:SetVisible(false)
  end
end

function _SetUpFlippingPoints(oWidget)
  oWidget.CustomData.bHavePoints = true
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local nMidY = (nY2 + nY1) * 0.5
  oWidget.CustomData.nStartPoint = oWidget:AddAnimationPoint({
    x = nX1,
    y = nY1,
    x2 = nX2,
    y2 = nY2
  })
  oWidget.CustomData.nClosePoint = oWidget:AddAnimationPoint({
    x = nX1,
    y = nMidY,
    x2 = nX2,
    y2 = nMidY
  })
end

function _AnimateFrameClose(oWidget, fCallback, tCallbackData)
  if not oWidget.CustomData.bHavePoints then
    _SetUpFlippingPoints(oWidget)
  end
  oWidget:AnimateToPoint(oWidget.CustomData.nClosePoint, 0.15, true, fCallback, tCallbackData)
end

function _AnimateFrameOpen(oWidget)
  if not oWidget.CustomData.bHavePoints then
    _SetUpFlippingPoints(oWidget)
  end
  oWidget:AnimateToPoint(oWidget.CustomData.nStartPoint, 0.15, true, _SetTextVisible, {oWidget, true})
end

function _SetTextVisible(oUnused, oWidget, bVisible)
  if oWidget.ParentWidget and oWidget.ParentWidget.ParentWidget then
    local tWidgets = oWidget.ParentWidget.ParentWidget:GetChildren()
    if tWidgets[2] then
      tWidgets[2].CustomData.bSuppressVisibilityChange = false
      tWidgets[2]:SetVisible(bVisible, true)
    end
    if tWidgets[3] then
      tWidgets[3].CustomData.bSuppressVisibilityChange = false
      tWidgets[3]:SetVisible(bVisible, true)
    end
    if tWidgets[4] then
      tWidgets[4].CustomData.bSuppressVisibilityChange = false
      tWidgets[4]:SetVisible(bVisible, true)
    end
  end
end

_knRotateTime = 0.5
_knRotateDelay = 0.05

function _InitializeRotationAnimation(oWidget)
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    if "image" == oChild.BasicData.type then
      local nRotation = oChild:GetRotation()
      oChild.CustomData.nOriginalRotation = nRotation
      if 1 == nIndex then
        oChild.CustomData.nPoint = oChild:AddAnimationPoint({
          nRotation = nRotation + 180,
          nRotationDirection = -1
        })
      else
        oChild.CustomData.nPoint = oChild:AddAnimationPoint({
          nRotation = nRotation + 180,
          nRotationDirection = 1
        })
      end
    end
  end
  oWidget.Animate = _AnimateBackgroundRotation
end

function _AnimateBackgroundRotation(oWidget)
  local oChild = oWidget:GetChildren()[2]
  oChild:AnimateToPoint(oChild.CustomData.nPoint, _knRotateTime, true, oChild.SetRotation, {
    oChild.CustomData.nOriginalRotation
  })
  Event.Create(Event.TimerRelative, {_knRotateDelay}, _AnimateNext, {oWidget, 3})
end

function _AnimateNext(oWidget, nIndex)
  local oChild = oWidget:GetChildren()[nIndex]
  local bLast = false
  if not oChild then
    oChild = oWidget:GetChildren()[1]
    bLast = true
  end
  if oChild then
    oChild:AnimateToPoint(oChild.CustomData.nPoint, _knRotateTime, true, oChild.SetRotation, {
      oChild.CustomData.nOriginalRotation
    })
    if not bLast then
      Event.Create(Event.TimerRelative, {_knRotateDelay}, _AnimateNext, {
        oWidget,
        nIndex + 1
      })
    end
  end
end

function HandleGunSwitchForAnimation(oWidget, tEvent)
  if tEvent.uNewCurrentGun then
    if oWidget.CustomData.bSuppress then
      oWidget.CustomData.bSuppress = false
      oWidget.CustomData.bWaitingForSupport = true
    else
      _BeginWeaponSwitchAnimation(oWidget, tEvent.uNewCurrentGun)
      oWidget.CustomData.bWaitingForSupport = false
    end
  end
end

function HandleExplosiveSwitchForAnimation(oWidget, tEvent)
  if tEvent.uNewCurrentExplosive then
    _BeginWeaponSwitchAnimation(oWidget, tEvent.uNewCurrentExplosive)
  end
end

function _WeaponSwitchAccessor(oWidget, uNewWeapon)
  if oWidget.CustomData.bWaitingForSupport then
    _BeginWeaponSwitchAnimation(oWidget, uNewWeapon)
    oWidget.CustomData.bWaitingForSupport = false
  end
end

function _BeginWeaponSwitchAnimation(oWidget, uNewWeapon)
  local tChildren = oWidget:GetChildren()
  if oWidget.CustomData.bAnimating then
    if oWidget.CustomData.nTime >= 0.2 then
      oWidget.CustomData.uNewWeapon = uNewWeapon
      _PerformIconSwitchAnimation(tChildren[1]:GetChildren()[1], oWidget.CustomData.uNewWeapon)
    else
      oWidget.CustomData.uNewWeapon = uNewWeapon
    end
  else
    oWidget:SetEventHandler("GuiUpdate", _UpdateControllingWidget)
    oWidget.CustomData.nTime = 0
    oWidget.CustomData.uNewWeapon = uNewWeapon
    oWidget.CustomData.bAnimating = true
    if "userdata" == type(oWidget.CustomData.uNewWeapon) or "string" == type(oWidget.CustomData.uNewWeapon) then
      local tElements = oWidget:GetChildren()[1]:GetChildren()
      local oCircle = tElements[2]
      local oBullets = tElements[4]
      oBullets:SetVisible(true)
      oCircle:SetVisible(true)
    end
    _AnimateBullet(tChildren[1]:GetChildren()[4], 1)
    if tChildren[2] then
      _SetUpCustomTextVisibility(tChildren[2])
      tChildren[2]:SetVisible(false)
      tChildren[2].CustomData.bSuppressVisibilityChange = true
    end
    if tChildren[3] then
      _SetUpCustomTextVisibility(tChildren[3])
      tChildren[3]:SetVisible(false)
      tChildren[3].CustomData.bSuppressVisibilityChange = true
    end
    if tChildren[4] then
      _SetUpCustomTextVisibility(tChildren[4])
      tChildren[4]:SetVisible(false)
      tChildren[4].CustomData.bSuppressVisibilityChange = true
    end
  end
end

function _SetUpCustomTextVisibility(oWidget)
  if not oWidget.RealSetVisible then
    oWidget.RealSetVisible = oWidget.SetVisible
    oWidget.SetVisible = _CustomSetVisible
  end
end

function _CustomSetVisible(oWidget, bVisible)
  if not oWidget.CustomData.bSuppressVisibilityChange then
    oWidget:RealSetVisible(bVisible)
  end
end

function _UpdateControllingWidget(oWidget, nDeltaTime)
  if not nDeltaTime then
    return
  end
  local tElements = oWidget:GetChildren()[1]:GetChildren()
  local oIcon = tElements[1]
  local oCircle = tElements[2]
  local oFrame = tElements[3]
  local oBullets = tElements[4]
  if not oWidget.CustomData.nTime then
    oWidget.CustomData.nTime = 0
  end
  local nPreviousTime = oWidget.CustomData.nTime
  local nNewTime = nPreviousTime + nDeltaTime
  if _PassedPoint(nPreviousTime, nNewTime, 0.05) then
    _AnimateBullet(oBullets, 2)
  end
  if _PassedPoint(nPreviousTime, nNewTime, 0.1) then
    _AnimateBullet(oBullets, 3)
  end
  if _PassedPoint(nPreviousTime, nNewTime, 0.15) then
    _AnimateBullet(oBullets, 4)
  end
  if _PassedPoint(nPreviousTime, nNewTime, 0.2) then
    _AnimateBullet(oBullets, 5)
    _PerformIconSwitchAnimation(oIcon, oWidget.CustomData.uNewWeapon)
  end
  if _PassedPoint(nPreviousTime, nNewTime, 0.25) then
    _AnimateBullet(oBullets, 0)
  end
  if _PassedPoint(nPreviousTime, nNewTime, 0.36) then
    if "userdata" == type(oWidget.CustomData.uNewWeapon) or "string" == type(oWidget.CustomData.uNewWeapon) then
      oFrame:SetVisible(true)
      _AnimateFrameClose(oFrame, _AnimateFrameOpen, {oFrame})
    else
      oBullets:SetVisible(false)
      oCircle:SetVisible(false)
      _AnimateFrameClose(oFrame, oFrame.SetVisible, {false})
    end
    oWidget:SetEventHandler("GuiUpdate", nil)
    oWidget.CustomData.bAnimating = false
  end
  oWidget.CustomData.nTime = nNewTime
end

function _AnimateBullet(oBullets, nNumber)
  nNumber = nNumber + 1
  oChild = oBullets:GetChildren()[nNumber]
  if oChild then
    oChild:AnimateToPoint(oChild.CustomData.nPoint, _knRotateTime, true, oChild.SetRotation, {
      oChild.CustomData.nOriginalRotation
    })
  end
end

function _PassedPoint(nPreviousValue, nNewValue, nPoint)
  if nPreviousValue < nNewValue then
    if nPreviousValue < nPoint and nPoint <= nNewValue then
      return true
    end
  elseif nNewValue < nPreviousValue and nPoint < nPreviousValue and nNewValue <= nPoint then
    return true
  end
  return false
end
