inherit("MrxTaskContract")
import("MrxSubtitle")
import("MrxLayerManager")
import("MrxUtil")
import("MrxSupportData")
import("MrxFactionManager")

function LoadAssets(self, tSaveData)
  local tLayersToAdd = {
    "VZ_state_gurcon005",
    "VZ_state_gurcon005_junglebase",
    "VZ_state_gurcon005_airportdefbase",
    "VZ_state_gurcon005_depot"
  }
  MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
end

function Activated(self)
  MrxTaskContract.Activated(self)
  TargetsKilled = 0
  self:CreateChild({
    sName = "Assassinate Universal Petroleum Targets",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "GurCon005_Target01",
      "GurCon005_Target02",
      "GurCon005_Target03",
      "GurCon005_Target04"
    },
    bDspBlp = true,
    sDspShortDesc = "[GurCon005.Objectives.005]",
    tOnPartComplete = {
      {
        TargetKilledVO,
        {self}
      }
    },
    tOnComplete = {
      {
        self.Complete,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    },
    vVoSeqOnAdd = {}
  })
end

function Reported(self)
  bNoReport = false
end

function TargetKilledVO(self)
  if TargetsKilled == 0 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Gur05-17"
    })
  elseif TargetsKilled == 1 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Gur05-18"
    })
  elseif TargetsKilled == 2 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Gur05-20"
    })
  elseif TargetsKilled == 3 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Gur05-21"
    })
  end
  TargetsKilled = TargetsKilled + 1
end
