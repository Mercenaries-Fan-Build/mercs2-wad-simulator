inherit("MecJob")
import("MrxUtil")

function Activated(self)
  self.sVehImg = "global_polaroid_belmont"
  self.sVehLabel = "rtr"
  self.sObjText = "[MecJob001.Objectives.001]"
  self.iMinHealth = 30
  self.sIntro = {
    "Fiona-In-Mission-Job-Mec01-02"
  }
  self.sWrongVeh = "Eva-In-Mission-Contract-Mech01-80"
  self.sRightVeh = "Fiona-In-Mission-Job-Mec01-03"
  local tLocs = {
    "mecjob001.rtrspawn1",
    "mecjob001.rtrspawn2",
    "mecjob001.rtrspawn3"
  }
  MecJob.Activated(self)
  local i = Math.randi(#tLocs)
  MrxUtil.SpawnObject("RTR (crappy)", tLocs[i])
end
