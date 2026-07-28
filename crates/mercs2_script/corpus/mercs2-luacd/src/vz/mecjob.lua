inherit("MrxTaskJob")
import("MrxGuiHudMessage")
import("MrxSoundCategories")
import("MrxMusic")
import("MrxVoSequence")

function LoadAssets(self, tSaveData)
  local tLayersToAdd = {
    "vz_state_gua_upperclass_pristine",
    "Vz_State_MecJob"
  }
  MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
end

function AssetsLoaded(self)
  self:_IssueAssetsLoadedCallbacks()
  self:_CreateEvent(Event.TimerRelative, {2}, self.Activated, {self})
end

function Activated(self)
  self.tMsgWrongVeh = {
    "Eva-In-Mission-Contract-Mech01-09",
    "Eva-In-Mission-Contract-Mech01-10",
    "Eva-In-Mission-Contract-Mech01-11"
  }
  self.tMsgLowHealth = {
    "Eva-In-Mission-Contract-Mech01-01",
    "Eva-In-Mission-Contract-Mech01-02",
    "Eva-In-Mission-Contract-Mech01-03",
    "Eva-In-Mission-Contract-Mech01-04"
  }
  self.sMsgGarageDestroyed = "Fiona-In-Mission-Contract-Mech01-62"
  self.sMsgExitGarage = "Eva-In-Mission-Contract-Mech01-61"
  self.inRegion = Pg.GetGuidByName("mechanicHQ.rgn.inside")
  self.outRegion = Pg.GetGuidByName("mechanicHQ.rgn.outside")
  self.garage = Pg.GetGuidByName("mechanicHQ")
  MrxTaskJob.Activated(self)
  local kPropVehName = "mc001.propVehicle"
  local o = Pg.GetGuidByName(kPropVehName)
  if o then
    Object.Remove(o)
  end
  if self.sPropVehTemplate then
    o = MrxUtil.SpawnObject(self.sPropVehTemplate, "meccon.loc.inprogress", kPropVehName)
  end
  self:_CreateEvent(Event.ObjectDeath, {
    self.garage
  }, _GarageDestroyed, {self})
  self:_PlayerOutside()
  self:_SetupJob()
end

function Complete(self)
  Sound.TransitionMusic("mission_success", true)
  local sPlayer1Name = Object.GetLocalizedName(Player.GetPrimaryCharacter())
  local sPlayer2Name
  local uPlayer2Guid = Player.GetSecondaryCharacter()
  if uPlayer2Guid then
    sPlayer2Name = Object.GetLocalizedName(uPlayer2Guid)
  end
  local tConfig = self:GetConfig()
  Hud.Fanfare:Create({
    sType = "mission",
    sProfileName1 = sPlayer1Name,
    sProfileName2 = sPlayer2Name,
    fCallback = function(bRepeat)
      MrxSoundCategories.Fade("fanfare", false)
      MrxTaskMission.Complete(self)
    end
  })
  MrxSoundCategories.Fade("fanfare", true)
  Hud.Fanfare:Commence({})
end

function Cancel(self)
  MrxMusic.PlayFanfare(false)
  local tConfig = self:GetConfig()
  Hud.Fanfare:Create({
    sType = "mission",
    sProfileName1 = "unused",
    bAllowRetry = bRetryable,
    sCancelMsg = self._sCancelMsg or "[Fanfare.Cancel.Msg]",
    fCallback = function(bRetry)
      MrxSoundCategories.Fade("fanfare", false)
      MrxTaskMission.Cancel(self)
      self:GetParent():Cancel()
    end
  })
  MrxSoundCategories.Fade("fanfare", true)
  Hud.Fanfare:Commence({})
end

function Cleanup(self)
  Object.CloseGate(self.garage)
  if self.objFilterVehFound then
    self.objFilterVehFound = nil
  end
  MrxTaskJob.Cleanup(self)
end

function CreateChild(self, tConfig)
  return MrxTaskMission.CreateChild(self, tConfig)
end

function _CreateDeliverObjective(self)
  local objName = "MecJob: Deliver " .. self.sVehLabel
  self:CreateChild({
    sName = objName,
    sModuleName = "MrxTaskObjectiveDeliver",
    sTgtLabelFilter = "vehicle",
    vDestLoc = "mechanicHQ_loc_delivery",
    fDist = 3,
    bStop = true,
    bXZOnly = false,
    sDspShortDesc = self.sObjText,
    bDspMsgUpd = false,
    bDisplayHelpText = true,
    bTrackOnActivate = false,
    nQuota = 1,
    vVoSeqOnAdd = self.sIntro,
    fOnComplete = function()
      MrxVoSequence.Start({
        self.sMsgExitGarage
      })
      self:_ExitGarage()
      self:_DeliveryTargetDestroyedBeforeExitingEvent()
    end,
    fOnCancel = function()
      self:Cancel()
    end,
    fEvaluateTarget = function(uDeliveredObj)
      return self:_EvaluateDeliveryTarget(uDeliveredObj)
    end
  })
  self.sIntro = nil
end

function _SetupJob(self)
  _DisplayVehicleImg(self.sVehImg, Player.GetPrimaryPlayer())
  _DisplayVehicleImg(self.sVehImg, Player.GetSecondaryPlayer())
  self:_CreateDeliverObjective()
  local oFilter = ObjectFilter.Create()
  ObjectFilter.SetFilter(oFilter, self.sVehLabel)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    oFilter,
    "a",
    "e"
  }, _VehicleFound, {self})
  self.objFilterVehFound = oFilter
end

function _EvaluateDeliveryTarget(self, uGuid)
  local result = false
  local uDriver = Vehicle.GetDriver(uGuid)
  if uDriver and not Object.IsPlayerControlled(uDriver) then
    return false
  end
  self.uVehicle = uGuid
  if Object.HasLabel(uGuid, self.sVehLabel) then
    if Object.GetHealth(uGuid) >= self.iMinHealth then
      result = true
    else
      self:_PlayRandomVO(self.tMsgLowHealth)
    end
  elseif not self.bFirstWrongVehWarning then
    self.bFirstWrongVehWarning = true
    self:_PlayOneVO(self.sWrongVeh)
  else
    self:_PlayRandomVO(self.tMsgWrongVeh)
  end
  return result
end

function _VehicleFound(self)
  self:_PlayOneVO(self.sRightVeh)
end

function _GarageDestroyed(self)
  MrxVoSequence.Start({
    self.sMsgGarageDestroyed,
    {
      Cancel,
      {self}
    }
  })
end

function _DeliveryTargetDestroyedBeforeExitingEvent(self)
  self._tEvents.ePostDeliveryDestroyed = Event.Create(Event.ObjectDeath, {
    self.uVehicle
  }, _DeliveryTargetDestroyedBeforeExiting, {self})
end

function _DeliveryTargetDestroyedBeforeExiting(self)
  Event.Delete(self._tEvents.eDoorTrigger)
  self._tEvents.eDoorTrigger = nil
  self:_PlayRandomVO(self.tMsgLowHealth)
  self:_CreateDeliverObjective()
  self:_PlayerInside()
end

function _PlayerOutside(self)
  Object.CloseGate(self.garage)
  Debug.Printf("PENDING CLEANUP ON ", self.uVehicle)
  if self.uVehicle then
    self._tEvents.eCleanupCar = Event.Create(Event.ObjectPhysicsEvent, {
      self.garage,
      "gateFullyClosed"
    }, _CleanupVehicle, {self})
  end
  self._tEvents.eDoorTrigger = Event.Create(Event.Boundary, {
    Player.GetAnyCharacter(),
    self.outRegion,
    "enter",
    false
  }, _PlayerInside, {self})
end

function _PlayerInside(self, uPCharacter)
  Object.OpenGate(self.garage)
  if self._tEvents.eCleanupCar then
    Event.Delete(self._tEvents.eCleanupCar)
    self._tEvents.eCleanupCar = nil
  end
  self._tEvents.eDoorTrigger = Event.Create(Event.Boundary, {
    Player.GetAllCharacters(),
    self.outRegion,
    "exit",
    false
  }, _PlayerOutside, {self})
end

function _ExitGarage(self)
  Event.Delete(self._tEvents.eDoorTrigger)
  self._tEvents.eDoorTrigger = Event.Create(Event.Boundary, {
    Player.GetAllCharacters(),
    self.outRegion,
    "exit",
    false
  }, _VehicleDelivered, {self})
end

function _VehicleDelivered(self)
  Debug.Printf("----------------- _VehicleDelivered: PLAYER OUTSIDE OF REGION")
  if Object.IsAlive(self.uVehicle) and Object.InsideBoundary(self.uVehicle, self.inRegion) then
    Object.CloseGate(self.garage)
    
    local function fCallback()
      self:Complete()
    end
    
    self._tEvents.eGateClosed = Event.Create(Event.ObjectPhysicsEvent, {
      self.garage,
      "gateFullyClosed"
    }, _CleanupVehicle, {self, fCallback})
    self._tEvents.eGateStuck = Event.Create(Event.ObjectPhysicsEvent, {
      self.garage,
      "gateStuck"
    }, _CleanupVehicle, {self, fCallback})
    self._tEvents.eMedevac = Event.Create(Event.ScriptEvent, {
      "MedevacComplete",
      function(tData)
        return true
      end
    }, _CleanupVehicle, {self, fCallback})
  else
    self:_CreateDeliverObjective()
    self:_PlayerOutside()
  end
end

function _CleanupVehicle(self, fCallback)
  Debug.Printf("@@@@@@@@@@ CLEANUP ON ", self.uVehicle)
  Event.Delete(self._tEvents.eGateClosed)
  Event.Delete(self._tEvents.eGateStuck)
  Event.Delete(self._tEvents.eMedevac)
  if self.uVehicle then
    if Object.OutsideBoundary(self.uVehicle, self.inRegion) then
      Debug.Printf("VEHICLE OUTSIDE BOUNDARY")
      return false
    end
    local tRiders = Vehicle.GetRiders(self.uVehicle)
    for i, uRider in pairs(tRiders) do
      if Object.IsPlayerControlled(uRider) then
        Debug.Printf("VEHICLE CONTAINS PLAYER")
        return false
      end
    end
    Object.Remove(self.uVehicle)
    self.uVehicle = nil
    Debug.Printf("@@@@@@@@@ REMOVED")
  end
  if type(fCallback) == "function" then
    fCallback()
  end
  return true
end

function _PlayRandomVO(self, tVOs)
  local i = math.randi(table.getn(tVOs))
  self:_PlayOneVO(tVOs[i])
end

function _PlayOneVO(self, sLine)
  if not self.bPlayingVO then
    self.bPlayingVO = true
    MrxVoSequence.Start({
      sLine,
      function()
        self.bPlayingVO = nil
      end
    })
  end
end

function _DisplayVehicleImg(sImg, uPlayer)
  MrxGuiHudMessage.ShowMessage(uPlayer, sImg, nil, nil, 320, 240, nil, nil, 224, 224, 4)
end
