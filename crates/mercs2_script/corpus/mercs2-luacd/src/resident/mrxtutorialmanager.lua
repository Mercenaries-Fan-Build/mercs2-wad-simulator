_tTutorials = {
  Swimming = {
    sModuleName = "WifTutorialSwimming"
  },
  WheeledVehicleBasic = {
    sModuleName = "WifTutorialWheeledVehicleBasic"
  },
  Boats = {
    sModuleName = "WifTutorialBoat"
  },
  Tanks = {
    sModuleName = "WifTutorialTank"
  },
  Helicopters = {
    sModuleName = "WifTutorialHelicopter"
  },
  C4 = {
    sModuleName = "WifTutorialC4"
  },
  CollateralDamage = {
    sModuleName = "WifTutorialCollateralDamage"
  },
  Trespass = {
    sModuleName = "WifTutorialTrespass"
  },
  NoFuel = {
    sModuleName = "WifTutorialNoFuel"
  },
  LowFuel = {
    sModuleName = "WifTutorialLowFuel"
  },
  Alarm = {
    sModuleName = "WifTutorialAlarm"
  },
  AirstrikeInterrupt = {
    sModuleName = "WifTutorialAirstrikeInterrupt"
  },
  HeliRepairPad = {
    sModuleName = "WifTutorialHeliRepairPad"
  },
  TankHijack = {
    sModuleName = "WifTutorialTankHijack"
  },
  CoopTether = {
    sModuleName = "WifTutorialCoopTether"
  },
  APC = {
    sModuleName = "WifTutorialAPC"
  },
  VehicleDisguise = {
    sModuleName = "WifTutorialVehicleDisguise"
  },
  Collectibles = {
    sModuleName = "WifTutorialCollectibles"
  },
  GateHonk = {
    sModuleName = "WifTutorialGateHonk"
  },
  C4Switch = {
    sModuleName = "WifTutorialC4Switch"
  },
  AlliesHonk = {
    sModuleName = "WifTutorialAlliesHonk"
  },
  CoopRevive = {
    sModuleName = "WifTutorialCoopRevive"
  }
}
_sCurrentActiveTutorial = nil

function Reset()
  _sCurrentActiveTutorial = nil
  for sName, tTutorialData in pairs(_tTutorials) do
    if tTutorialData.oTutorial then
      tTutorialData.oTutorial:DestroyEvents()
    end
  end
  HideMessage(true)
end

function Setup()
  Reset()
  for sName, tTutorialData in pairs(_tTutorials) do
    if not tTutorialData.bComplete then
      local function _TutorialModuleImported(tModule)
        local oTutorial = tModule:Create({sName = sName})
        
        tTutorialData.oTutorial = oTutorial
        oTutorial:SetupActivationCriteria()
      end
      
      dynamic_import(tTutorialData.sModuleName, _TutorialModuleImported)
    end
  end
end

function BeginCustomTutorial(sIdentifierName, bDontNetSync)
  if Sys.TutorialsEnabled() == false then
    return
  end
  if _sCurrentActiveTutorial then
    return
  end
  _sCurrentActiveTutorial = sIdentifierName
end

function EndCustomTutorial(sIdentifierName, bDontNetSync)
  if _sCurrentActiveTutorial and _sCurrentActiveTutorial ~= sIdentifierName then
    return
  end
  HideMessage(bDontNetSync, sIdentifierName)
  _sCurrentActiveTutorial = nil
end

function StartTutorial(sTutorialName, bDontNetSync)
  if _tTutorials[sTutorialName] and _tTutorials[sTutorialName].bComplete then
    return
  end
  if _tTutorials[sTutorialName] and _tTutorials[sTutorialName].oTutorial then
    _tTutorials[sTutorialName].oTutorial:ActivateTutorial(bDontNetSync)
  end
end

function SetCurrentTutorial(oTutorial, bDontNetSync)
  if Sys.TutorialsEnabled() == false then
    return false
  end
  local sName = oTutorial:GetName()
  if _bMessageDisplayed then
    return false
  end
  local bResult = ShowMessage(oTutorial:GetMessage(), bDontNetSync, sName)
  return bResult
end

function UpdateCurrentTutorial(oTutorial, bDontNetSync)
  if oTutorial then
    if _sCurrentActiveTutorial and _sCurrentActiveTutorial ~= oTutorial:GetName() then
      return false
    end
    local sName = oTutorial:GetName()
    _sCurrentActiveTutorial = nil
    local bResult = ShowMessage(oTutorial:GetMessage(), bDontNetSync, sName)
    _sCurrentActiveTutorial = sName
    return bResult
  else
    return false
  end
end

function HideCurrentTutorial(oTutorial, bComplete, bDontNetSync)
  local sName = oTutorial:GetName()
  if _sCurrentActiveTutorial and sName ~= _sCurrentActiveTutorial then
    return
  end
  HideMessage(bDontNetSync, sName)
  _sCurrentActiveTutorial = nil
  _tTutorials[sName].bComplete = bComplete
  return true
end

function GetTutorial(sTutorial)
  return _tTutorials[sTutorial].oTutorial
end

function DestroyTutorial(oTutorial)
  local sName = oTutorial:GetName()
  local tTutorialData = _tTutorials[sName]
  local sModuleName = tTutorialData.sModuleName
  dynamic_remove(sModuleName)
  oTutorial = nil
  tTutorialData.oTutorial = nil
end

_sCurrentMessage = nil

function ShowMessage(sMessage, bDontNetSync, sIdentifierName)
  if Net.IsServer() and not bDontNetSync then
    Net.SetTutorialMessage(sMessage)
  end
  if Sys.TutorialsEnabled() == false then
    return false
  end
  if _sCurrentMessage and _sCurrentMessage == sMessage then
    Debug.Printf("Warning: adding the same tutorial message while it is showing: " .. sMessage)
    return false
  end
  if _sCurrentActiveTutorial and sIdentifierName ~= _sCurrentActiveTutorial and sIdentifierName then
    return false
  end
  _sCurrentActiveTutorial = sIdentifierName
  Hud.Tutorial:SetText({
    vPlayer = Player.GetLocalPlayer(),
    sText = sMessage
  })
  _bMessageDisplayed = true
  _sCurrentMessage = sMessage
  return true
end

function HideMessage(bDontNetSync, sIdentifierName)
  if _sCurrentActiveTutorial and _sCurrentActiveTutorial ~= sIdentifierName then
    return
  end
  if Net.IsServer() and not bDontNetSync then
    Net.SetTutorialMessage()
  end
  Hud.Tutorial:SetText({
    vPlayer = Player.GetLocalPlayer(),
    sText = nil
  })
  _bMessageDisplayed = false
  _sCurrentMessage = nil
  if not sIdentifierName then
    _sCurrentActiveTutorial = nil
  end
end

function SaveSingleton()
  local tSaveData = {}
  for sName, tTutorialData in pairs(_tTutorials) do
    if tTutorialData.bComplete then
      table.insert(tSaveData, sName)
    end
  end
  return tSaveData
end

function LoadSingleton(tSaveData)
  for i, sName in ipairs(tSaveData) do
    local tTutorialData = _tTutorials[sName]
    if tTutorialData then
      tTutorialData.bComplete = true
    end
  end
end
