inherit("MrxTaskContract")
import("MrxTaskObjectiveDeliver")
import("MrxLayerManager")
import("MrxTimer")
import("MrxSubtitle")
import("MrxAchievements")
import("MrxTutorialManager")
import("MrxFactionManager")
import("MrxMusic")

function Activated(self)
  MrxTaskContract.Activated(self)
  Debug.Printf("**********CHICON008: Activated")
  MrxLayerManager.Remove({
    "vz_state_cumana_act1CHI"
  })
  local spawnZtz98 = Pg.GetGuidByName("loc_ChiCon008_ZTZ98")
  local uYaw = Object.GetYaw(spawnZtz98)
  local x, y, z = Object.GetPosition(spawnZtz98)
  chineseRaceTank = Pg.Spawn("ZTZ98", x, y, z, uYaw)
  Vehicle.Usable(chineseRaceTank, false)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    chineseRaceTank,
    "d",
    "e"
  }, MrxMusic.PlaySpecialMusic, {
    "mu_mission_chicon008_01"
  })
  nTimeLimit = 60
  nTimeToAdd = 15
  local nCompletions = self:GetNumCompletions()
  if nCompletions == 1 then
    nTimeLimit = 50
    nTimeToAdd = 10
  elseif 2 <= nCompletions then
    nTimeLimit = 40
    nTimeToAdd = 10
  end
  StartRace(self, tVehicle, nTimeLimit, nTimeToAdd)
  local tankDeath = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("ChiCon008_ZTZ98")
  }, self.Cancel, {self})
  RaceTutorial(self)
end

function RaceTutorial(self)
  MrxTutorialManager.ShowMessage("[ChiCon008.Terms.Tutorial]")
end

function StartRace(self, tVehicle, nTime, nAddTime)
  local sMissionName = "ChiCon008"
  FionaVoChiCon008(self)
  self:CreateChild({
    sName = "TankRace",
    sModuleName = "MrxTaskObjectiveDestroy",
    bIsDestruction = true,
    sDspBlpWldIcon = "HUD_objective_timer",
    bDspBlpRdr = false,
    bDspBlpPda = false,
    bOptional = true,
    nAddTime = nTimeToAdd,
    bTrackOnActivate = false,
    vTgtInclude = {
      "destroy_barrel_01",
      "destroy_barrel_02",
      "destroy_barrel_03",
      "destroy_barrel_04",
      "destroy_barrel_05",
      "destroy_barrel_06",
      "destroy_barrel_07",
      "destroy_barrel_08",
      "destroy_barrel_09",
      "destroy_barrel_010",
      "destroy_barrel_011",
      "destroy_barrel_012",
      "destroy_barrel_013",
      "destroy_barrel_014",
      "destroy_barrel_015",
      "destroy_barrel_016",
      "destroy_barrel_017",
      "destroy_barrel_018",
      "destroy_barrel_019"
    },
    fOnPartComplete = function()
      self:ObjectDestroyed()
    end,
    fOnCancel = function()
      self:RaceFailed()
    end
  })
  self.oTankRaceObjective = self:CreateChild({
    sRaceMission = "ChiCon008",
    sName = "ZTZ98 Race",
    sModuleName = "MrxTaskRace",
    tTimerParams = {nStartTime = nTime},
    vTgtInclude = chineseRaceTank,
    sDspShortDesc = "[ChiCon008.Objectives.001]",
    tCourseLocs = {
      sMissionName .. "_Checkpoint01",
      sMissionName .. "_Checkpoint02",
      sMissionName .. "_Checkpoint002",
      sMissionName .. "_Checkpoint03",
      sMissionName .. "_Checkpoint04",
      sMissionName .. "_Checkpoint004",
      sMissionName .. "_Checkpoint05",
      sMissionName .. "_Checkpoint005",
      sMissionName .. "_Checkpoint06",
      sMissionName .. "_Checkpoint006",
      sMissionName .. "_Checkpoint07",
      sMissionName .. "_Checkpoint007",
      sMissionName .. "_Checkpoint08",
      sMissionName .. "_Checkpoint008",
      sMissionName .. "_Checkpoint09",
      sMissionName .. "_Checkpoint009",
      sMissionName .. "_Checkpoint10",
      sMissionName .. "_Checkpoint0010",
      sMissionName .. "_Checkpoint11",
      sMissionName .. "_Checkpoint12",
      sMissionName .. "_Checkpoint13"
    },
    fOnComplete = function()
      local uWinner = self.oTankRaceObjective._uWinner
      if uWinner then
        MrxAchievements.AchievementAddCount("ACHIEVEMENT_HIGHWAY_TO_HELL", 1, uWinner, true)
      end
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Pmc01-10",
        3,
        {
          self.Complete,
          {self}
        }
      })
    end,
    fOnCancel = function()
      self:RaceFailed()
    end
  })
end

function ObjectDestroyed(self)
  MrxTutorialManager.HideMessage(self)
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 2,
    sText = "[green]" .. tostring(nTimeToAdd) .. " " .. "[ChiCon008.Terms.Bonus]"
  })
  if self.oTankRaceObjective then
    self.oTankRaceObjective._oTimer:AddTime(nTimeToAdd)
  end
  self:_CreateEvent(Event.TimerRelative, {2}, function()
    Hud.ObjectiveTray:SetSlotToText({nSlot = 2, sText = " "})
  end)
end

function FionaVoChiCon008(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Chi08-01"
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("ChiCon008_Checkpoint07"),
    "<",
    60,
    false,
    false
  }, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Chi08-02"
    })
  end)
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("ChiCon008_Checkpoint04"),
    "<",
    60,
    false,
    false
  }, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi08-04",
      2,
      {
        mattias = "Mattias-In-Mission-Contract-Chi08-06",
        jennifer = "Jennifer-In-Mission-Contract-Chi08-07",
        chris = "Chris-In-Mission-Contract-Chi08-08"
      }
    })
  end)
end

function VehicleUnentered(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi08-02",
    3,
    {
      self.Cancel,
      {self}
    }
  })
  self:_SetCancelMessage("[ChiCon008.Terms.Cancel01]")
end

function RaceFailed(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi08-03",
    3,
    {
      self.Cancel,
      {self}
    }
  })
  self:_SetCancelMessage("[ChiCon008.Terms.Cancel02]")
end

function Cleanup(self)
  Hud.ObjectiveTray:SetSlotToText({nSlot = 1, sText = " "})
  Hud.ObjectiveTray:SetSlotToText({nSlot = 2, sText = " "})
  Object.Remove(chineseRaceTank)
  MrxMusic.StopSpecialMusic()
  MrxLayerManager.Add({
    "vz_state_cumana_act1CHI"
  })
  MrxTaskContract.Cleanup(self)
end
