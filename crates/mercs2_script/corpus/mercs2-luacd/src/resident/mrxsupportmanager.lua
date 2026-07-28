import("MrxSupport")
import("MrxPmc")
CurrentlyEquippedSupport = {}

function CurrentlyEquippedSupport:AddSupport(oSupport)
  local uCharacterGuid = Player.GetCharacter(oSupport.uOwner)
  self[uCharacterGuid] = oSupport
end

SupportQueue = {}

function SupportQueue:Add(uPlayerGuid, oSupport, uDesignatorGuid)
  if not self[uPlayerGuid] then
    self[uPlayerGuid] = {}
  end
  if not self[uPlayerGuid][oSupport] then
    self[uPlayerGuid][oSupport] = {}
  end
  local bFound = false
  for nIndex, uGuid in pairs(self[uPlayerGuid][oSupport]) do
    if uGuid == uDesignatorGuid then
      bFound = true
    end
  end
  if not bFound then
    table.insert(self[uPlayerGuid][oSupport], uDesignatorGuid)
  end
end

function SupportQueue:GetSupport(uPlayerGuid, uDesignatorGuid)
  local tSupportList = self[uPlayerGuid]
  if not tSupportList then
    return nil
  end
  for oSupport in pairs(tSupportList) do
    if "table" == type(tSupportList[oSupport]) then
      for nIndex in pairs(tSupportList[oSupport]) do
        if tSupportList[oSupport][nIndex] == uDesignatorGuid then
          return oSupport
        end
      end
    end
  end
  return nil
end

ValidationQueue = {}

function ValidationQueue:Add(uPlayerGuid, oSupport, uDesignatorGuid, nX, nY, nZ)
  local oValidator = {}
  oValidator.uPlayerGuid = uPlayerGuid
  oValidator.oSupport = oSupport
  oValidator.uDesignatorGuid = uDesignatorGuid
  oValidator.nX = nX
  oValidator.nY = nY
  oValidator.nZ = nZ
  table.insert(self, oValidator)
  if table.getn(self) < 2 then
    self:_Process()
  end
end

function ValidationQueue:_Process()
  local oValidator = self[1]
  oValidator.oSupport.oDesignator.fValidationFunction(_ValidationQueueCallback, oValidator.nX, oValidator.nY, oValidator.nZ, oValidator.oSupport)
end

function _ValidationQueueCallback(bSuccess, nX, nY, nZ)
  ValidationQueue:_Callback(bSuccess, nX, nY, nZ)
end

function ValidationQueue:_Callback(bSuccess, nX, nY, nZ)
  local oValidator
  if table.getn(self) > 0 then
    oValidator = self[1]
    table.remove(self, 1)
  else
    return nil
  end
  if not bSuccess then
    Airstrike.RefillDesignator(oValidator.oSupport.oDesignator.uWeaponGuid)
    if type(nX) == "string" then
      MrxSupport.DenialMessage(nX)
    else
      MrxSupport.DenialMessage("nodrop")
    end
    oValidator.oSupport.oDesignator:OnDeny(oValidator.uDesignatorGuid)
  else
    oValidator.oSupport:GetDesignator():SetDesignationParameters(nX, nY, nZ, oValidator.uDesignatorGuid)
    CompleteDesignation(oValidator.oSupport, oValidator.uDesignatorGuid, oValidator.uPlayerGuid)
  end
  if table.getn(self) > 0 then
    self:_Process()
  end
end

function CompleteDesignation(oSupport, uDesignatorGuid, uPlayerGuid)
  if not uPlayerGuid then
    return
  end
  if not oSupport then
    return
  end
  if CurrentlyEquippedSupport[uPlayerGuid] and CurrentlyEquippedSupport[uPlayerGuid] == oSupport then
    CurrentlyEquippedSupport[uPlayerGuid] = nil
    Airstrike.RemoveDesignator(uPlayerGuid)
  else
    SupportQueue[uPlayerGuid][oSupport] = nil
  end
  if not IsRecruitAvailable(oSupport:GetRecruit()) then
    oSupport.oDesignator:OnDeny(uDesignatorGuid)
    return
  end
  if oSupport:GetFuelCost() > MrxPmc.GetFuelQty() and not oSupport.bUnrestrictedByFuel then
    oSupport.oDesignator:OnDeny(uDesignatorGuid)
    MrxSupport.DenialMessage("fuel")
    return
  end
  oSupport:GetDesignator():SetDesignationParameters(nil, nil, nil, uDesignatorGuid)
  if oSupport:GetRecruit() == "Copter" then
    StartRecruitCooldown("Copter", -1)
  else
    StartRecruitCooldown(oSupport:GetRecruit())
  end
  oSupport:GetDesignator():CompleteDesignation()
end

function OnActivate(uDesignatorGuid, uPlayerGuid)
  Event.Create(Event.ObjectHibernation, {uDesignatorGuid, "awake"}, FinishOnActivate, {uDesignatorGuid})
end

function FinishOnActivate(uDesignatorGuid)
  local uPlayerGuid
  if Airstrike.FindDesignatorOwner then
    uPlayerGuid = Airstrike.FindDesignatorOwner(uDesignatorGuid)
  end
  local oSupport
  if uPlayerGuid then
    oSupport = CurrentlyEquippedSupport[uPlayerGuid]
  end
  if not oSupport then
    return
  end
  SupportQueue:Add(uPlayerGuid, oSupport, uDesignatorGuid)
end

function OnInitialize(uDesignatorGuid, uPlayerGuid)
  OnActivate(uDesignatorGuid, uPlayerGuid)
end

function OnDesignate(uDesignatorGuid, uPlayerGuid, uTargetGuid, bSuccess, bDeactivation)
  if not uPlayerGuid then
    return
  end
  if Object.IsAttached(uPlayerGuid, uDesignatorGuid) then
    Airstrike.RemoveDesignator(uPlayerGuid)
    return
  end
  if "table" == type(bDeactivation) or "number" == type(bDeactivation) then
    bDeactivation = false
  end
  if "table" == type(bSuccess) or "number" == type(bSuccess) then
    bSuccess = true
  end
  if "table" == type(uTargetGuid) or "number" == type(uTargetGuid) then
    uTargetGuid = nil
  end
  local oSupport = SupportQueue:GetSupport(uPlayerGuid, uDesignatorGuid)
  if not oSupport then
    return
  end
  if not oSupport:GetDesignator().bDesignateOnDeath and bDeactivation then
    return
  end
  if oSupport.oDesignator.bDesignated then
    return
  end
  oSupport.oDesignator.bDesignated = true
  local nX, nY, nZ = Object.GetPosition(uDesignatorGuid)
  oSupport:GetDesignator():SetDesignationParameters(nX, nY, nZ, uDesignatorGuid, uTargetGuid)
  if oSupport.oDesignator and MrxSupport.TestAALevel(oSupport.oDesignator.sAATestLevel) then
    Airstrike.RefillDesignator(oSupport.oDesignator.uWeaponGuid)
    oSupport.oDesignator:OnDeny(uDesignatorGuid)
    MrxSupport.DenialMessage(oSupport.oDesignator.sAATestLevel)
    return
  end
  if oSupport.oDesignator and "function" == type(oSupport.oDesignator.fValidationFunction) then
    ValidationQueue:Add(uPlayerGuid, oSupport, uDesignatorGuid, nX, nY, nZ)
  else
    CompleteDesignation(oSupport, uDesignatorGuid, uPlayerGuid)
  end
end

function OnDeactivate(uDesignatorGuid, uPlayerGuid, uTargetGuid, bSuccess, bDeactivation)
  if "userdata" ~= type(uDesignatorGuid) then
    uDesignatorGuid = nil
  end
  if "userdata" ~= type(uPlayerGuid) then
    uPlayerGuid = nil
  end
  OnDesignate(uDesignatorGuid, uPlayerGuid, uTargetGuid, bSuccess, bDeactivation)
end

function OnTimer(uDesignatorGuid, uPlayerGuid, uTargetGuid, bSuccess, bDeactivation)
  if "userdata" ~= type(uDesignatorGuid) then
    uDesignatorGuid = nil
  end
  if "userdata" ~= type(uPlayerGuid) then
    uPlayerGuid = nil
  end
  OnDesignate(uDesignatorGuid, uPlayerGuid, uTargetGuid, bSuccess, bDeactivation)
end

function OnDeath(uDesignatorGuid, uPlayerGuid)
end

_tRecruitStates = false
_tRecruitTimers = false
_nDefaultCooldownTime = 12

function IsRecruitAvailable(sRecruit)
  local recruitHash = String.GetHash(sRecruit)
  if nil == _tRecruitStates[recruitHash] then
    RegisterRecruit(sRecruit)
  end
  return _tRecruitStates[recruitHash]
end

function StartRecruitCooldown(sRecruit, nTime)
  local recruitHash = String.GetHash(sRecruit)
  Debug.Printf(tostring(sRecruit) .. " unavailable!")
  if nil ~= _tRecruitStates[recruitHash] then
    nTime = nTime or _nDefaultCooldownTime
    _tRecruitStates[recruitHash] = false
    Net.SendCustomEvent("MrxSupportManager", NETEVENT_RECRUITSTATE, {sRecruit, 0}, true)
    if 0 < nTime then
      Net.SendCustomEvent("MrxSupportManager", NETEVENT_STARTTIMER, {sRecruit, nTime}, true)
      local oTimer = _tRecruitTimers[recruitHash]
      oTimer:SetTotalTime(nTime)
      oTimer:Reset()
      oTimer:SetCallback(MakeRecruitAvailable, {sRecruit})
      oTimer:Start()
    end
    return nTime
  end
  return nil
end

function RegisterRecruit(sRecruit)
  local recruitHash = String.GetHash(sRecruit)
  if nil == _tRecruitStates[recruitHash] then
    _tRecruitStates[recruitHash] = true
    Net.SendCustomEvent("MrxSupportManager", NETEVENT_RECRUITSTATE, {sRecruit, 1}, true)
    _tRecruitTimers[recruitHash] = SupportTimer:Create()
  end
end

function MakeRecruitAvailable(sRecruit)
  Debug.Printf(tostring(sRecruit) .. " available!")
  local recruitHash = String.GetHash(sRecruit)
  if nil ~= _tRecruitStates[recruitHash] then
    _tRecruitStates[recruitHash] = true
    Net.SendCustomEvent("MrxSupportManager", NETEVENT_RECRUITSTATE, {sRecruit, 1}, true)
  end
  Event.Post("RecruitAvailable", {sRecruit})
end

function GetRecruitTimes(sRecruit)
  local recruitHash = String.GetHash(sRecruit)
  if _tRecruitTimers[recruitHash] and false == _tRecruitStates[recruitHash] then
    if _tRecruitTimers[recruitHash].oEventHandle then
      return _tRecruitTimers[recruitHash]:GetElapsedTime(), _tRecruitTimers[recruitHash]:GetTotalTime()
    else
      return -2, -1
    end
  else
    return nil, nil
  end
end

NETEVENT_RECRUITSTATE = 0
NETEVENT_STARTTIMER = 1

function NetEventCallback(nType, tArgs)
  if nType == NETEVENT_RECRUITSTATE then
    local recruitHash = tArgs[1]
    if nil == _tRecruitStates[recruitHash] then
      _tRecruitTimers[recruitHash] = SupportTimer:Create()
    end
    if tArgs[2] == 1 then
      _tRecruitStates[recruitHash] = true
    else
      _tRecruitStates[recruitHash] = false
    end
  elseif nType == NETEVENT_STARTTIMER then
    local recruitHash = tArgs[1]
    if nil == _tRecruitStates[recruitHash] then
      _tRecruitTimers[recruitHash] = SupportTimer:Create()
    end
    local oTimer = _tRecruitTimers[recruitHash]
    oTimer:SetTotalTime(tArgs[2])
    oTimer:Reset()
    oTimer:Start()
  end
end

SupportTimer = {
  nElapsedTime = 0,
  nTotalTime = 10,
  oEventHandle = nil,
  fCallback = nil,
  tCallbackData = {}
}

function SupportTimer:Create()
  NewTimer = {
    nElapsedTime = 0,
    nTotalTime = 10,
    oEventHandle = nil,
    fCallback = nil,
    tCallbackData = {}
  }
  setmetatable(NewTimer, self)
  self.__index = self
  return NewTimer
end

function SupportTimer:Delete()
  self:Stop()
end

function SupportTimer:GetElapsedTime()
  return self.nElapsedTime
end

function SupportTimer:GetTotalTime()
  return self.nTotalTime
end

function SupportTimer:SetTotalTime(nTotalTime)
  self.nTotalTime = nTotalTime or self.nTotalTime
end

function SupportTimer:Reset()
  self.nElapsedTime = 0
end

function SupportTimer:SetCallback(fCallback, tCallbackData)
  self.fCallback = fCallback or self.fCallback
  self.tCallbackData = tCallbackData or self.tCallbackData
end

function SupportTimer:Start()
  if not self.oEventHandle then
    if self.nElapsedTime >= self.nTotalTime then
      self:Reset()
    end
    if Event.GuiGameTimer then
      self.oEventHandle = Event.CreatePersistent(Event.GuiGameTimer, {}, self._EventCallback, {self})
    else
      self.oEventHandle = Event.CreatePersistent(Event.GuiUpdate, {}, self._EventCallback, {self})
    end
  end
end

function SupportTimer:Stop()
  if self.oEventHandle then
    Event.Delete(self.oEventHandle)
    self.oEventHandle = nil
  end
end

function SupportTimer:_EventCallback(nDeltaTime)
  self.nElapsedTime = self.nElapsedTime + nDeltaTime
  if self.nElapsedTime >= self.nTotalTime then
    self:Stop()
    if self.fCallback then
      local tData = self.tCallbackData or {}
      self.fCallback(unpack(tData))
    end
  end
end

function Init()
  _tRecruitStates = {}
  _tRecruitTimers = {}
end
